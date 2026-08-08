# Invalid Sign-In Link Feedback — Design Spec

**Date:** 2026-08-07
**Status:** Draft

## Overview

When a user clicks an expired or already-used email magic-link,
`GET /api/auth/verify` redirects to `/?error=invalid_token` (auth.rs:203-206).
Today the frontend ignores this param and renders the normal showings page with
no explanation — the user has no idea why login didn't happen. This spec adds a
minimal landing page that explains the situation.

## Behavior

`?error=invalid_token` renders `InvalidLinkPage` instead of the showings page.

| Viewer | State | Message |
|--------|-------|---------|
| Anyone with an expired/used link | not logged in | "This sign-in link has expired or was already used. Please request a new one on the device where you want to sign in." |

No action button, no login form, no mount poll. Deliberately: in the
cross-device flow the clicking device is not the device that should request a
new link — offering a "request new link" action there would redirect the next
login to the wrong device.

## Frontend changes

### New: `frontend/src/pages/InvalidLinkPage.tsx`

- Renders the brand header only — logo (`/projector-logo.svg`, class
  `marquee-logo`), title (`t("brand")`), tagline (`t("tagline")`) in the
  existing `marquee-brand` / `marquee-text` classes. No nav, no login panel,
  no sidebar, no showings content.
- Below the header, `<p className="auth-note">{t("auth.invalidLink")}</p>`.
- No `useEffect`, no poll.

### Modify: `frontend/src/App.tsx`

Add a branch: when `searchParams.get("error") === "invalid_token"` render
`InvalidLinkPage`. Order: `error` check before the `login=confirmed` check is
irrelevant (they never co-occur), but keep `AuthProvider` wrapping all
branches.

### Modify: `frontend/src/locales/{en,de}.json`

- en `auth.invalidLink`: "This sign-in link has expired or was already used. Please request a new one on the device where you want to sign in."
- de `auth.invalidLink`: "Dieser Anmelde-Link ist abgelaufen oder wurde bereits verwendet. Bitte fordere einen neuen Link auf dem Gerät an, auf dem du dich anmelden möchtest."

## Testing

### New: `frontend/src/pages/InvalidLinkPage.test.tsx`

- Renders the brand header (logo + title) and the invalid-link message.
- No showings content (no `Megaplex PlusCity`, no `Impressum`).
- No sign-in form / no "request new link" action present.

### Modify: `frontend/src/App.test.tsx`

- `/?error=invalid_token` renders the invalid-link message, not the showings
  page.

## Out of scope

- SSO error params (`invalid_state`, `oauth_failed`) — they also redirect to
  `/?error=...` but remain unhandled.
- Backend changes — none needed.
