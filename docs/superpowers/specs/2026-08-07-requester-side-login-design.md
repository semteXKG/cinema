# Requester-Side Login for Email Magic Link — Design Spec

**Date:** 2026-08-07
**Status:** Draft

## Overview

Today the email magic-link session cookie is issued to whichever browser
*clicks* the verify link. If the link is opened on a phone, the phone gets the
session and the desktop that requested it does not. This spec makes the
**requesting** device the one that gets logged in.

The fix introduces a pending-request + poll flow: the requesting browser holds
an `ov_pending` cookie, clicking the email link only marks the request
"confirmed", and the requesting browser's poll then creates the session. Email
ownership remains the security proof — the `ov_pending` cookie alone cannot
log anyone in.

## Behavior

| Scenario | Result |
|----------|--------|
| Request on desktop, click link on desktop | Desktop logs in (via poll, ~3s) |
| Request on desktop, click link on mobile | Desktop logs in; mobile sees "confirmed" page, no session |
| Two pending requests | Independent tokens/cookies; only the one whose link was clicked completes |
| Link never clicked | Poll stays `false` until the 15-min token expires |

Rules:
- **Only the requesting device logs in.** `verify` never creates a session.
- Email access is the sole proof of identity. `ov_pending` (a cookie containing
  the token) is not sufficient by itself — a session is only issued once the
  email token is marked used.
- No fast path for same-device clicks — uniform code path, one way to test.
- No cap on concurrent pending requests — each request carries its own token.

## Backend changes (`backend/src/auth.rs`, `backend/src/db.rs`)

### DB: new query

`db::lookup_email_token(pool, token) -> Result<Option<EmailTokenState>>` where
`EmailTokenState { email: String, used: bool }`. Read-only SELECT on
`email_tokens` (`used` is already tracked; `consume_email_token` already exists
and does the single-use UPDATE).

### POST /api/auth/email — set `ov_pending` cookie

After `insert_email_token`, also set:

```
ov_pending=<token>; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=900
```

Same token as the email link. Response stays `200 {"ok":true}`.

### GET /api/auth/verify — confirm only, no session

Consumes the token exactly as today (`consume_email_token`), but instead of
creating a user + session + cookie, redirects:

- Token valid → `302` to `{base_url}/?login=confirmed`
- Token invalid/expired/used → `302` to `{base_url}/?error=invalid_token`

No `ov_session` cookie is ever set here.

### GET /api/auth/login/status — new poll endpoint

Reads the `ov_pending` cookie. Behavior:

| State | Response |
|-------|----------|
| No `ov_pending` cookie | `200 {"loggedIn":false}` |
| Token not used, not expired | `200 {"loggedIn":false}` |
| Token used, not expired | Create user (`find_or_create_user("email", email, email)`), create session, set `ov_session` cookie, clear `ov_pending`, return `200 {"loggedIn":true}` |
| Token expired (never used) | Clear `ov_pending`, return `200 {"loggedIn":false}` |

### Router

Add `.route("/api/auth/login/status", get(get_login_status))`.

## Frontend changes

### `api.ts`

Add `fetchLoginStatus(): Promise<boolean>` — GET `/api/auth/login/status`,
returns the `loggedIn` field.

### `useAuth.tsx`

`loginEmail(email)` becomes polling: call `sendMagicLink(email)`, then poll
`fetchLoginStatus()` every 3s (with a 15-min ceiling). Resolve when
`loggedIn` is true; call `refresh()` so the user state updates. Reject/stop on
expiry or network error.

### `Marquee.tsx`

- After submit, show a waiting state ("Check your email — waiting for
  confirmation…") instead of the static "sent" state.
- Read `?login=confirmed` from the URL (on mount, via `useSearchParams`) and
  show a banner "Sign-in confirmed — you can close this tab."
- On poll success the header flips to "Sign out" automatically via `useAuth`.

### `locales/{en,de}.json`

New `auth` keys:

- `auth.waiting` — en: "Check your email — waiting for confirmation…" / de:
  "Prüfe deine E-Mails — warte auf Bestätigung…"
- `auth.confirmed` — en: "Sign-in confirmed — you can close this tab." / de:
  "Anmeldung bestätigt — du kannst diesen Tab schließen."

## Security considerations

- `ov_pending` is HttpOnly (not JS-readable), Secure, SameSite=Lax, 15-min TTL.
- A session is only created when the email token is consumed, so possessing
  `ov_pending` without clicking the link grants nothing.
- Token remains single-use with 15-min expiry — unchanged.
- The verify endpoint now performs strictly less (never issues a session), so
  the attack surface is reduced, not increased.

## Testing

Backend (`auth.rs` tests, `#[sqlx::test]`):
- `verify` consumes the token and does **not** set a session cookie; response
  is a 302 to `/?login=confirmed`.
- `verify` with bad/used token → 302 to `/?error=invalid_token`.
- `login/status` without cookie → `{loggedIn:false}`.
- `login/status` with pending cookie before click → `{loggedIn:false}`.
- Full flow: insert token, set cookie, consume via verify, then login/status →
  `{loggedIn:true}` + `ov_session` cookie present + `ov_pending` cleared.
- `login/status` with expired token → `{loggedIn:false}` + cookie cleared.

Frontend (`Marquee.test.tsx` / `App.test.tsx`):
- Submit shows waiting state; mock `fetchLoginStatus` → true flips to signed-in.
- `?login=confirmed` renders the confirmation banner.
- i18n keys resolve in both locales.

## Out of scope

- Logging in from the clicked (non-requesting) device.
- Sign-out everywhere / device management.
- Rate limiting on the email endpoint.
