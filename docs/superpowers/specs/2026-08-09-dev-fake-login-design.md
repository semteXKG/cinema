# Dev Fake Login — Design Spec

**Date:** 2026-08-09
**Status:** Draft

## Overview

Add a way to sign in on a local development box where no email/SMTP or SSO
providers are configured. A dev-only backend endpoint mints a **real session**
through the existing `find_or_create_user` + `create_session` + `ov_session`
cookie path; the frontend shows a "Dev: sign in as dev@ov.local" button in the
login modal. The endpoint is gated behind an explicit `FAKE_LOGIN` env var so
it can never appear in production (the Helm chart never sets it).

## Behavior

| Trigger | Result |
|---------|--------|
| `FAKE_LOGIN` unset/empty/`0` | `/api/auth/dev-login` returns 404; `providers.dev` is `false`; no button in the modal |
| `FAKE_LOGIN=1` (or `true`) | `/api/auth/dev-login` creates the `dev@ov.local` user if absent, mints a 30-day session, sets the `ov_session` cookie, redirects to `/` |
| Click "Dev: sign in as dev@ov.local" (modal) | Browser navigates to `/api/auth/dev-login`, lands back on `/` logged in |
| Sign out | Existing logout flow unchanged |

Notes:
- The dev-login redirect target is the host-relative path `/`, **not**
  `BASE_URL` (which defaults to the prod URL). This makes it work from the
  Vite dev server (:5173) and the backend-served SPA (:8080) alike.
- The `ov_session` cookie keeps its `Secure` attribute; modern browsers treat
  `localhost` as a secure context, so it is accepted over http on localhost.

## Backend

### Modify: `backend/src/config.rs`

- `Config` gains `pub fake_login: bool`.
- Parse from `FAKE_LOGIN`: enabled when the value is exactly `1` or `true`
  (case-insensitive); absent or empty → `false`. Empty strings are already
  filtered by the existing `get` wrapper.
- Tests: default is `false`; `"1"` → true; `"true"` → true; `"0"`/`"false"`/
  absent → false.

### Modify: `backend/src/web.rs`

- `AppState` gains `pub fake_login: bool`.
- All construction sites updated: `backend/src/main.rs:118`,
  `backend/src/web.rs:419/507/545/606`, `backend/src/auth.rs:749`.

### Modify: `backend/src/main.rs`

- Pass `fake_login: config.fake_login` into `web::AppState`.

### Modify: `backend/src/auth.rs`

- `ProvidersResponse` gains `dev: bool` (serde `camelCase` → `dev`);
  `get_providers` returns `dev: state.fake_login`.
- New handler `get_dev_login`:
  - `const DEV_EMAIL: &str = "dev@ov.local"`.
  - If `!state.fake_login` → `404`.
  - Else: `user_id = db::find_or_create_user(&pool, "dev", DEV_EMAIL, DEV_EMAIL)`,
    `session_token = new_token()`, `expires = Utc::now() + Duration::days(SESSION_DAYS)`,
    `db::create_session(...)`, respond
    `redirect_to_with_cookie("/", &build_session_cookie(&session_token).await)`.
  - Registered always: `GET /api/auth/dev-login`.
- Backend tests (`auth.rs`):
  - `providers_endpoint` asserts `dev == false` (existing test extended).
  - `providers_endpoint_reports_dev_login` — `fake_login: true` → `dev == true`.
  - `dev_login_disabled_returns_404`.
  - `dev_login_creates_session_and_redirects` — 302, `Location: /`,
    `Set-Cookie` starts with `ov_session=`; then `/api/auth/me` with that
    cookie returns 200 with email `dev@ov.local`.
  - `test_state` returns `fake_login: false`; add a `test_state_dev` helper.

### Modify: `docker-compose.dev.yml`

- Backend service environment gains `FAKE_LOGIN: ${FAKE_LOGIN:-1}` (the dev
  stack defaults it on; set `FAKE_LOGIN=0` to disable).

## Frontend

### Modify: `frontend/src/types.ts`

- `AuthProviders` gains `dev: boolean`.

### Modify: `frontend/src/components/LoginModal.tsx`

- Below the SSO buttons render:
  `{providers?.dev && <button className="auth-provider" onClick={() => { window.location.href = "/api/auth/dev-login"; }}><span>{t("auth.devLogin")}</span></button>}`

### Modify: `frontend/src/locales/{en,de}.json`

- `auth.devLogin`: en `"Dev: sign in as dev@ov.local"`,
  de `"Dev: als dev@ov.local anmelden"`.

### Modify: `frontend/src/components/LoginModal.test.tsx`

- `beforeEach` provider mock gains `dev: false` (the typed mock now requires
  the field).
- `afterEach` gains `vi.unstubAllGlobals()` (for the location stub).
- New tests:
  - Button shown when `dev: true`.
  - Button hidden when `dev: false`.
  - Clicking navigates to `/api/auth/dev-login` (assert via a stubbed
    `window.location`).

### Modify: `frontend/src/App.test.tsx`, `frontend/src/pages/PreferencesPage.test.tsx`

- Provider mock bodies gain `dev: false` for consistency (no behavior change).

## Documentation

### Modify: `LOCAL_DEV.md`

- New "Fake login (local development)" section: how to enable (`FAKE_LOGIN=1`
  or rely on the dev compose default), and how to use it (open the login modal
  → "Dev: sign in as dev@ov.local"). Notes that `providers.dev` only reports
  true when the backend has the flag set.

## Out of scope

- Changing the real auth flows (email/SMTP, Google, GitHub).
- Any production deployment changes (the Helm chart is untouched and never
  sets `FAKE_LOGIN`).
- Preferences persistence — this feature only enables the logged-in UI in dev.
