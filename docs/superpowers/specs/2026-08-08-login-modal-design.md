# Login Modal Rework — Design Spec

**Date:** 2026-08-08
**Status:** Draft

## Overview

Replace the current inline login panel (a horizontal strip that drops under
the nav in `Marquee.tsx:55-88`) with a centered modal dialog. The modal shows
the three login options stacked vertically: **email first**, then a visual
separator, then the SSO buttons (Google, GitHub — Apple appears if ever
configured). Dismissal via overlay click, Escape, and a close button.

## Behavior

| Trigger | Result |
|---------|--------|
| Click "Sign in" (nav, not logged in) | Modal opens |
| Click overlay outside the dialog | Modal closes |
| Press Escape while open | Modal closes |
| Click the × button | Modal closes |
| Email submitted | Modal shows the waiting state ("Check your email — waiting for confirmation…") and stays open; when the poll completes (`user` flips non-null) the modal closes |
| SSO clicked | Page navigates to `/api/auth/sso/{provider}` (existing behavior) |

The `?login=confirmed` landing page and `?error=invalid_token` page are
separate routes and are unaffected.

## Component structure

### New: `frontend/src/components/LoginModal.tsx`

Props:
```ts
interface LoginModalProps {
  open: boolean;
  onClose: () => void;
}
```

Renders, when `open`:
- A full-screen overlay `<div className="modal-overlay">` (click → `onClose`).
- A centered `<div className="modal" role="dialog" aria-modal="true">`:
  - Close button `<button className="modal-close" aria-label="Close">×</button>`.
  - Title `<h2>{t("auth.signIn")}</h2>`.
  - **Email section** (if `providers?.email`): the existing email form
    (input + submit) with the existing waiting state — copied from the
    current `Marquee.tsx:57-71` logic, including `sending` +
    `t("auth.waiting")`.
  - **Separator** `<div className="modal-divider">{t("auth.or")}</div>`.
  - **SSO section**: config-driven buttons exactly as today
    (`Marquee.tsx:72-86`) — Google, Apple (if configured), GitHub.
- A `useEffect` that adds a `keydown` listener for Escape (→ `onClose`) when
  `open`, removing it on cleanup.
- A `useEffect`: when `user` becomes non-null while open, call `onClose()`
  (login via the email poll completed).
- Uses `useAuth()` internally for `{ user, providers, loginEmail, loginSSO }`.

The email form submit handler (`handleEmailSubmit`) and `sending` state live
inside the modal (moved out of Marquee).

### Modify: `frontend/src/components/Marquee.tsx`

- Keep the `showLogin` state and the "Sign in"/"Sign out" button logic
  (`Marquee.tsx:43-53`), but change the Sign-in button to
  `onClick={() => setShowLogin(true)}`.
- Remove the inline `auth-panel` block (`Marquee.tsx:55-88`) and its
  `emailInput` / `sending` / `handleEmailSubmit` state and `useAuth`
  destructure entries that become unused (`loginEmail`, `providers`).
- Render `<LoginModal open={showLogin} onClose={() => setShowLogin(false)} />`
  inside the `<header>` (or after it).

### Modify: `frontend/src/index.css`

Add modal styles:
- `.modal-overlay`: `position:fixed; inset:0; background:rgba(0,0,0,.55);
  display:flex; align-items:center; justify-content:center; z-index:100;
  padding:1rem`.
- `.modal`: `background:var(--panel); border:1px solid var(--edge);
  border-radius:8px; padding:1.2rem 1.4rem; max-width:320px; width:100%;
  text-align:center; position:relative`.
- `.modal-close`: positioned top-right, styled like `.auth-btn`.
- `.modal-divider`: `margin:.9rem 0; color:var(--dim); font-size:.75rem;
  display:flex; align-items:center; gap:.6rem` with `::before`/`::after`
  flex lines using `var(--edge)`.
- Reuse existing `.auth-*` classes for the form, input, submit, and SSO
  buttons; stack the email form and SSO buttons vertically inside the modal
  (`.modal form`, `.modal .auth-sso` `display:block; width:100%;
  margin-top:.5rem`).
- `.auth-note` is reused for the waiting message if needed.

### Modify: `frontend/src/locales/{en,de}.json`

Add `auth.or`:
- en: `"or"`
- de: `"oder"`

(Reuse `auth.signIn`, `auth.signOut`, `auth.emailPlaceholder`,
`auth.sendLink`, `auth.waiting`, `auth.signInWith` unchanged.)

## Testing

### New: `frontend/src/components/LoginModal.test.tsx`

Render `<AuthProvider><LoginModal open onClose={vi.fn()} /></AuthProvider>`
with mocked `../api`:
- Renders the email input and the SSO buttons (Google + GitHub when
  configured; Apple absent when `apple:false`).
- Clicking the overlay calls `onClose`.
- Pressing Escape calls `onClose`.
- Clicking the × button calls `onClose`.
- Submitting an email shows the waiting text (`auth.waiting`) while
  `fetchLoginStatus` resolves `false`.

### Modify: `frontend/src/components/Marquee.test.tsx`

- Update the "shows login panel with email and Google SSO buttons" test to
  assert the **modal** opens (email input + "Sign in with Google" present)
  after clicking "Sign in".
- Update the waiting-state test to match the modal.
- Keep sign-in-button, sign-out, and logout tests unchanged.

## Out of scope

- Changing the auth backend or `useAuth` API (`loginEmail`, `loginSSO`,
  `logout`, `pollLoginStatus` stay as-is).
- Styling the landing pages (`LoginConfirmedPage`, `InvalidLinkPage`) — the
  modal is separate from those.
- Adding new providers.
