# User Auth — Design Spec

**Date:** 2026-08-05
**Status:** Draft

## Overview

Add user authentication to the OV-Kino Linz app. Two login methods: email magic link and Google SSO. No passwords stored. Session-based auth via HTTP-only cookie. Minimal blast radius — the worst an attacker can do is change a user's cinema filter preferences (to be added later). This spec covers auth only, not preferences or notifications.

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
  provider    TEXT NOT NULL,        -- 'google' or 'email'
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
base_url:             String,          // BASE_URL, default "https://cinema.k-labs.app"
google_client_id:     Option<String>,  // GOOGLE_CLIENT_ID
google_client_secret: Option<String>,  // GOOGLE_CLIENT_SECRET
smtp_host:            Option<String>,  // SMTP_HOST
smtp_port:            u16,             // SMTP_PORT, default 587
smtp_username:        Option<String>,  // SMTP_USERNAME
smtp_password:        Option<String>,  // SMTP_PASSWORD
smtp_from:            Option<String>,  // SMTP_FROM
```

## API Endpoints

Base path: `https://cinema.k-labs.app`

| Method | Path | Auth? | Request | Response |
|--------|------|-------|---------|----------|
| `POST` | `/api/auth/email` | No | `{"email":"u@x.com"}` | `200 {"ok":true}` (always, to avoid email enumeration) |
| `GET`  | `/api/auth/verify` | No | `?token=...` | `302` redirect to `/` (sets session cookie on success) |
| `GET`  | `/api/auth/sso/google` | No | — | `302` redirect to Google consent screen |
| `GET`  | `/api/auth/sso/google/callback` | No | `?code=...&state=...` | `302` redirect to `/` (sets session cookie on success) |
| `GET`  | `/api/auth/me` | Yes | — | `200 {"id":1,"email":"u@x.com"}` or `401` |
| `POST` | `/api/auth/logout` | Yes | — | `200 {"ok":true}`, clears cookie, deletes session row |

### Email magic link flow

1. Frontend sends `POST /api/auth/email` with `{"email":"user@example.com"}`
2. Backend generates a 32-byte random token (URL-safe base64), stores it in `email_tokens` with 15-minute expiry
3. Backend sends email via SMTP with a link to `{BASE_URL}/api/auth/verify?token={token}`
4. User clicks link → backend validates token (not expired, not used), marks it `used=true`, creates/links user and identity `(provider=email, provider_id=user@example.com)`, creates a session, sets cookie, redirects to `{BASE_URL}/`
5. Always returns `200` from step 1 regardless of whether the email is registered — no enumeration

### Google SSO flow

1. `GET /api/auth/sso/google` → backend builds Google OAuth URL with `client_id`, `redirect_uri={BASE_URL}/api/auth/sso/google/callback`, `scope=openid email`, `state=<random>` (stored in a short-lived cookie for CSRF protection), redirects user
2. Google redirects back → backend validates `state`, exchanges `code` for token, fetches userinfo (`sub`, `email`, `email_verified`)
3. If `email_verified` is false, reject
4. Create/link user and identity `(provider=google, provider_id=sub)`, create session, set cookie, redirect to `/`

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
- `401` → user is not logged in. Show login UI: email input + "Send magic link" button, and (if `GOOGLE_CLIENT_ID` is configured) "Sign in with Google" button.

### Login UI

Both login options live in the `<Marquee>` header area (a small text link or icon in the nav strip), or as a small dropdown/panel. Not a separate page — the app is public content, auth just enables customization.

### Logout

`POST /api/auth/logout` → clear local auth state → show login UI.

## Implementation notes

- New Rust crate dependency: `lettre` for SMTP email sending
- New Rust crate dependency: `oauth2` for Google OAuth2 client
- No new frontend npm dependencies needed
- Session cleanup: a periodic task prune expired sessions (runs alongside the checker loop, e.g. once per hour)
- Auth is optional: if no `SMTP_HOST` or no `GOOGLE_CLIENT_ID` is configured, those login methods are simply hidden from the frontend and their endpoints return `501 Not Implemented`
- Helmet/production: Helm chart gains new `secrets` fields: `googleClientId`, `googleClientSecret`, `smtpPassword`. SMTP config added to ConfigMap.

## Out of scope (future specs)

- User preferences (cinema filters, format filters, notification schedule)
- Telegram identity linking
- Email digests
- GitHub SSO
- Account deletion / GDPR
