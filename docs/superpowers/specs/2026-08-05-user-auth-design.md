# User Auth — Design Spec

**Date:** 2026-08-05
**Status:** Draft

## Overview

Add user authentication to the OV-Kino Linz app. Login methods: email magic link, Google, Apple, and GitHub SSO. No passwords stored. Session-based auth via HTTP-only cookie. Minimal blast radius — the worst an attacker can do is change a user's cinema filter preferences (to be added later). This spec covers auth only, not preferences or notifications.

## Database

Migration `0002_users.sql`:

```sql
CREATE TABLE users (
  id         BIGSERIAL PRIMARY KEY,
  email      TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE user_identities (
  user_id     BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  provider    TEXT NOT NULL,        -- 'google', 'apple', 'github', or 'email'
  provider_id TEXT NOT NULL,        -- Google 'sub' claim or email address
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (provider, provider_id)
);

CREATE TABLE sessions (
  token      TEXT PRIMARY KEY,
  user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE email_tokens (
  token      TEXT PRIMARY KEY,
  email      TEXT NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL,
  used       BOOLEAN NOT NULL DEFAULT false
);
```

**Identity model:** A `users` row represents one person. `user_identities` links auth methods to that row. Login always matches on `(provider, provider_id)` — never on email. Multiple identities (Google + email, or two emails) can point to the same user.

**Linking rule:** On login, if `(provider, provider_id)` matches an existing identity, use that user. If no identity match but a `users` row exists with the same `email`, add the new identity to that user. Otherwise create a new user.

## Config

New `Config` fields (all optional, auth features degrade gracefully when absent):

```rust
base_url:              String,          // BASE_URL, default "https://cinema.k-labs.app"
google_client_id:      Option<String>,  // GOOGLE_CLIENT_ID
google_client_secret:  Option<String>,  // GOOGLE_CLIENT_SECRET
apple_client_id:       Option<String>,  // APPLE_CLIENT_ID (Service ID)
apple_team_id:         Option<String>,  // APPLE_TEAM_ID
apple_key_id:          Option<String>,  // APPLE_KEY_ID
apple_private_key:     Option<String>,  // APPLE_PRIVATE_KEY (ES256 private key PEM)
github_client_id:      Option<String>,  // GITHUB_CLIENT_ID
github_client_secret:  Option<String>,  // GITHUB_CLIENT_SECRET
smtp_host:             Option<String>,  // SMTP_HOST
smtp_port:             u16,             // SMTP_PORT, default 587
smtp_username:         Option<String>,  // SMTP_USERNAME
smtp_password:         Option<String>,  // SMTP_PASSWORD
smtp_from:             Option<String>,  // SMTP_FROM
```

Apple's `client_secret` is not a static string — it's a JWT (ES256) signed with the private key, with `iss=team_id`, `sub=client_id`, `aud=appleid.apple.com`, 180-day expiry, generated at runtime via `jsonwebtoken`.

## API Endpoints

Base path: `https://cinema.k-labs.app`

| Method | Path | Auth? | Request | Response |
|--------|------|-------|---------|----------|
| `POST` | `/api/auth/email` | No | `{"email":"u@x.com"}` | `200 {"ok":true}` (always, to avoid email enumeration) |
| `GET`  | `/api/auth/verify` | No | `?token=...` | `302` redirect to `/` (sets session cookie on success) |
| `GET`  | `/api/auth/sso/google` | No | — | `302` redirect to Google consent screen |
| `GET`  | `/api/auth/sso/google/callback` | No | `?code=...&state=...` | `302` redirect to `/` (sets session cookie on success) |
| `GET`  | `/api/auth/sso/apple` | No | — | `302` redirect to Apple consent screen |
| `GET`  | `/api/auth/sso/apple/callback` | No | `?code=...&state=...` | `302` redirect to `/` (sets session cookie on success) |
| `GET`  | `/api/auth/sso/github` | No | — | `302` redirect to GitHub consent screen |
| `GET`  | `/api/auth/sso/github/callback` | No | `?code=...&state=...` | `302` redirect to `/` (sets session cookie on success) |
| `GET`  | `/api/auth/me` | Yes | — | `200 {"id":1,"email":"u@x.com"}` or `401` |
| `POST` | `/api/auth/logout` | Yes | — | `200 {"ok":true}`, clears cookie, deletes session row |
| `GET`  | `/api/auth/providers` | No | — | `200 {"email":true,"google":true,"apple":false,"github":true}` — which login methods are configured on the backend

### Email magic link flow

1. Frontend sends `POST /api/auth/email` with `{"email":"user@example.com"}`
2. Backend generates a 32-byte random token (URL-safe base64), stores it in `email_tokens` with 15-minute expiry
3. Backend sends email via SMTP with a link to `{BASE_URL}/api/auth/verify?token={token}`
4. User clicks link → backend validates token (not expired, not used), marks it `used=true`, creates/links user and identity `(provider=email, provider_id=user@example.com)`, creates a session, sets cookie, redirects to `{BASE_URL}/`
5. Always returns `200` from step 1 regardless of whether the email is registered — no enumeration

### SSO flow (Google, Apple, GitHub)

All three follow the same OAuth2 authorization code flow. Provider-specific details:

1. `GET /api/auth/sso/{provider}` → backend builds the provider's OAuth URL with `client_id`, `redirect_uri={BASE_URL}/api/auth/sso/{provider}/callback`, `state=<random>` (stored in a short-lived cookie for CSRF protection), redirects user.

2. Provider redirects back → backend validates `state`, exchanges `code` for token, fetches identity from the provider's userinfo endpoint.

3. Identity mapping per provider:

| Provider | `provider_id` | Email source | Notes |
|----------|--------------|--------------|-------|
| Google   | `sub` claim  | `email` (only if `email_verified`) | `scope=openid email` |
| Apple    | `sub` claim  | `email` from the first-use ID token; subsequent logins don't return email | Apple may use private relay email; that's fine — it becomes the identity |
| GitHub   | `id` (numeric user ID) | `GET /user/emails` (primary verified) | `scope=user:email` |

4. Create/link user and identity `(provider={provider}, provider_id=...)`, create session, set cookie, redirect to `/`.

**Library choice:** Google and Apple use the `openidconnect` crate — it does discovery, auth-code URL building, token exchange, id_token/JWKS signature verification, and UserInfo in one dependency (it wraps `oauth2`). GitHub is plain OAuth2 (no OIDC discovery/JWKS), so its `/user` + `/user/emails` calls are manual `reqwest`.

**Apple notes:** Apple is a standard OIDC provider — discovery at `https://appleid.apple.com/.well-known/openid-configuration`, JWKS at `https://appleid.apple.com/auth/keys`. The id_token signature is verified against Apple's JWKS (via `openidconnect`'s `CoreIdTokenVerifier`), and its `sub` claim is the identity. `email` is only present on the initial authorization — store it on first login; subsequent logins look up the existing identity by `sub`. Apple requires registering a Service ID and a private key for client secret generation (JWT-signed via `jsonwebtoken`).

### Session cookie

- Name: `ov_session`
- Flags: `HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=2592000` (30 days)
- Value: 32-byte random token (URL-safe base64), opaque lookup key
- On each authenticated request: `SELECT user_id FROM sessions WHERE token = $1 AND expires_at > now()`

### Auth extractor

An axum extractor (e.g. `AuthUser`) that reads the `ov_session` cookie, looks up the session, and injects `(user_id, email)` into the handler. Returns `401` if no valid session.

## Frontend

### Auth state detection

On mount, call `GET /api/auth/me`. Two states:
- `200` → user is logged in. Store `{ id, email }` in context/state. Show email + "Sign out" button.
- `401` → user is not logged in. Show login UI: email input + "Send magic link" button, and "Sign in with Google/Apple/GitHub" buttons (each only shown if its client ID is configured).

### Login UI

Both login options live in the `<Marquee>` header area (a small text link or icon in the nav strip), or as a small dropdown/panel. Not a separate page — the app is public content, auth just enables customization.

### Logout

`POST /api/auth/logout` → clear local auth state → show login UI.

## Implementation notes

- New Rust crate dependency: `lettre` for SMTP email sending
- New Rust crate dependency: `openidconnect` for Google + Apple (OIDC discovery, token exchange, id_token/JWKS verification)
- New Rust crate dependency: `jsonwebtoken` for Apple client secret JWT generation
- No new frontend npm dependencies needed
- Session cleanup: a periodic task prune expired sessions (runs alongside the checker loop, e.g. once per hour)
- Auth is optional: each login method is only available if its corresponding env vars are configured. Unconfigured methods are hidden from the frontend and their endpoints return `501 Not Implemented`
- Helmet/production: Helm chart gains new `secrets` fields: `googleClientId`, `googleClientSecret`, `appleClientId`, `appleTeamId`, `appleKeyId`, `applePrivateKey`, `githubClientId`, `githubClientSecret`, `smtpPassword`. SMTP config added to ConfigMap.

## Out of scope (future specs)

- User preferences (cinema filters, format filters, notification schedule)
- Telegram identity linking
- Email digests
- Account deletion / GDPR
