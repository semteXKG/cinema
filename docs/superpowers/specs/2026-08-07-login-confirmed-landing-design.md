# Login-Confirmed Landing Page — Design Spec

**Date:** 2026-08-07
**Status:** Draft

## Overview

When a user clicks an email magic-link, `GET /api/auth/verify` redirects to
`/?login=confirmed`. Today that renders the full showings page (Marquee +
sidebar + cinema sections + meta footer) with a small "confirmed" banner in
the header. This spec replaces that with a dedicated minimal landing page:
just the brand header (logo + title + tagline) and an adaptive message. The
landing page is also the recovery point for the same-device login poll, which
the SPA reload destroys.

## Behavior

`?login=confirmed` renders `LoginConfirmedPage` instead of the showings page.

| Viewer | State | Message |
|--------|-------|---------|
| Same device (mount poll succeeds) | logged in | "You have been logged in — you can close this window." |
| Other device / poll not yet done | not logged in | "Sign-in confirmed on your other device — you can close this window." |

The landing page runs the same mount poll as today
(`pollLoginStatus(undefined, 20s)`), so same-device clicks complete the
login. On a device with no `ov_pending` cookie the poll returns false
immediately and the page stays on the "other device" message.

## Frontend changes

### New: `frontend/src/pages/LoginConfirmedPage.tsx`

- Renders the brand header only — logo (`/projector-logo.svg`), title
  (`t("brand")`), tagline (`t("tagline")`) — reusing the existing
  `marquee-brand` / `marquee-logo` / `marquee-text` CSS classes. No nav, no
  login panel, no language switcher, no sidebar, no showings content.
- Below the header, an adaptive message:
  - `user != null` → `t("auth.loggedIn")`
  - else → `t("auth.confirmed")`
- On mount, if `user == null && !loading`, run
  `pollLoginStatus(undefined, 20_000, () => cancelled)` with cleanup, exactly
  as `Marquee.tsx:16-23` does today. On success the `user` flips non-null via
  the existing `refresh()` inside `pollLoginStatus`, and the message switches
  to the logged-in variant.

### Modify: `frontend/src/App.tsx`

At the root route, branch on the query param: when
`searchParams.get("login") === "confirmed"` render `LoginConfirmedPage`,
otherwise `ShowingsPage`. Use `useSearchParams` from `react-router-dom`.

### Modify: `frontend/src/components/Marquee.tsx`

Remove the now-dead confirmed banner and its mount poll (Marquee.tsx:16-23
and 66-70). `?login=confirmed` no longer renders the Marquee, so this logic
moves to `LoginConfirmedPage`. `pollLoginStatus` stays in the auth context
(used by both `loginEmail` and the new page).

### Modify: `frontend/src/locales/{en,de}.json`

- Reuse `auth.confirmed` (adjust wording to "...you can close this window.")
- Add `auth.loggedIn`:
  - en: "You have been logged in — you can close this window."
  - de: "Du bist angemeldet — du kannst dieses Fenster schließen."

## Testing

### New: `frontend/src/pages/LoginConfirmedPage.test.tsx`

- Renders the brand header (logo + title) and no showings content.
- Not logged in, no poll success → shows the "other device" message
  (`auth.confirmed`).
- Mount poll succeeds (mock `fetchLoginStatus` true) → shows the
  logged-in message (`auth.loggedIn`).
- i18n keys resolve in both locales.

### Modify: `frontend/src/components/Marquee.test.tsx`

Remove the confirmed-banner test (now covered by the landing page tests).
The remaining login/logout/SSO tests stay.

## Out of scope

- Changing the poll interval or deadline.
- Backend changes — none needed; this is purely presentational.
