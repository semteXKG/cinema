# Login Modal Frosted Glass + Official SSO Buttons — Design Spec

**Date:** 2026-08-08
**Status:** Draft

## Overview

Polish the login modal: give its panel a milkglass (frosted-glass) effect so
the page content shows through behind it, and replace the plain bordered
Google/GitHub buttons with brand-faithful white buttons carrying the official
provider logo SVGs.

This plan runs AFTER the Apple SSO removal plan (Apple is gone; the `.auth-sso`
CSS class is already removed, and Google/GitHub are the only SSO buttons).

## Behavior

- The modal panel is translucent frosted glass (`backdrop-filter: blur`) over
  the dark overlay; the overlay stays dark for contrast.
- Google and GitHub buttons are white with their official logo mark + "Sign
  in with Google/GitHub" (existing `auth.signInWith` i18n).

## Frontend changes

### `frontend/src/index.css`

- `.modal` becomes frosted:

```css
.modal{background:rgba(28,22,17,.72);backdrop-filter:blur(10px) saturate(1.2);
  -webkit-backdrop-filter:blur(10px) saturate(1.2);
  border:1px solid rgba(232,179,77,.35);border-radius:8px;
  padding:1.2rem 1.4rem;max-width:320px;width:100%;text-align:center;
  position:relative}
```

- New `.auth-provider` (replaces the removed `.auth-sso` for Google/GitHub):

```css
.auth-provider{display:flex;align-items:center;justify-content:center;
  gap:.6rem;width:100%;margin-top:.5rem;background:#fff;color:#3c4043;
  border:1px solid #d0d4d9;border-radius:6px;padding:.5rem .8rem;
  font-size:.85rem;font-weight:500;cursor:pointer}
.auth-provider:hover{background:#f8f9fa;border-color:#b8bcc1}
.auth-provider svg{width:1.1rem;height:1.1rem;flex:0 0 auto}
```

### `frontend/src/components/LoginModal.tsx`

- Add inline `GoogleIcon` (official 4-color G, `viewBox="0 0 48 48"`, paths
  `#EA4335` / `#4285F4` / `#FBBC05` / `#34A853`) and `GitHubIcon` (octocat,
  `viewBox="0 0 16 16"`, fill `#181717`). Both rendered with `aria-hidden`.
- Google and GitHub buttons become `.auth-provider` with logo + label:

```tsx
<button className="auth-provider" onClick={() => loginSSO("google")}>
  <GoogleIcon />
  <span>{t("auth.signInWith", { provider: "Google" })}</span>
</button>
```

## Out of scope

- i18n changes (none needed — `auth.signInWith` reused).
- Apple (removed by the preceding plan).
