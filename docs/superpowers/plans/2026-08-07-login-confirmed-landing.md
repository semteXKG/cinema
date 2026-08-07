# Login-Confirmed Landing Page — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render a minimal brand-only landing page for `?login=confirmed` instead of the full showings page, with an adaptive message that reflects whether the current browser got logged in.

**Architecture:** A new `LoginConfirmedPage` renders just the brand header (logo + title + tagline) and an adaptive message. `App.tsx` branches on the `login=confirmed` query param. The page runs the existing `pollLoginStatus(undefined, 20s)` mount poll (recovery point for same-device logins where the SPA reload kills the original poll). The Marquee's dead confirmed banner and mount poll are removed.

**Tech Stack:** React 19 + Vite + TypeScript, react-router-dom, react-i18next, Vitest + Testing Library.

## Global Constraints

- `?login=confirmed` renders `LoginConfirmedPage` — NOT the showings page (no sidebar, no cinema sections, no meta footer, no nav, no login panel, no language switcher).
- The landing page shows the brand header only: logo (`/projector-logo.svg`, class `marquee-logo`), title (`t("brand")`), tagline (`t("tagline")`) inside the existing `marquee-brand` / `marquee-text` classes.
- Adaptive message: `user != null` → `t("auth.loggedIn")`; otherwise → `t("auth.confirmed")`.
- i18n exact strings:
  - `auth.confirmed` en: "Sign-in confirmed — you can close this window." (wording change from "this tab"), de: "Anmeldung bestätigt — du kannst dieses Fenster schließen."
  - `auth.loggedIn` en: "You have been logged in — you can close this window.", de: "Du bist angemeldet — du kannst dieses Fenster schließen."
- Run frontend tests from `frontend/` with `npm test`; build with `npm run build`.
- No backend changes.

---

### Task 1: i18n keys and new `LoginConfirmedPage` component with tests

**Files:**
- Create: `frontend/src/pages/LoginConfirmedPage.tsx`
- Modify: `frontend/src/locales/en.json`, `frontend/src/locales/de.json`
- Create: `frontend/src/pages/LoginConfirmedPage.test.tsx`

**Interfaces:**
- Consumes: `useAuth()` context (`{ user, loading, pollLoginStatus }`), `useTranslation()`, existing CSS classes `marquee-brand`, `marquee-logo`, `marquee-text`, `auth-note`.
- Produces: `export function LoginConfirmedPage()` — brand header + adaptive message + mount poll.

- [ ] **Step 1: Add the i18n keys**

`frontend/src/locales/en.json` — change `auth.confirmed` value to `"Sign-in confirmed — you can close this window."` and add `"loggedIn": "You have been logged in — you can close this window."` after it:

```json
"waiting": "Check your email — waiting for confirmation…",
"confirmed": "Sign-in confirmed — you can close this window.",
"loggedIn": "You have been logged in — you can close this window.",
"signInWith": "Sign in with {{provider}}"
```

`frontend/src/locales/de.json` — change `auth.confirmed` value to `"Anmeldung bestätigt — du kannst dieses Fenster schließen."` and add `"loggedIn": "Du bist angemeldet — du kannst dieses Fenster schließen."`:

```json
"waiting": "Prüfe deine E-Mails — warte auf Bestätigung…",
"confirmed": "Anmeldung bestätigt — du kannst dieses Fenster schließen.",
"loggedIn": "Du bist angemeldet — du kannst dieses Fenster schließen.",
"signInWith": "Anmelden mit {{provider}}"
```

- [ ] **Step 2: Write the failing tests**

Create `frontend/src/pages/LoginConfirmedPage.test.tsx`:

```tsx
import { render, screen, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { LoginConfirmedPage } from "./LoginConfirmedPage";
import { AuthProvider } from "../hooks/useAuth";
import i18n from "../i18n";
import * as api from "../api";

vi.mock("../api");

const mockFetchMe = vi.mocked(api.fetchMe);
const mockFetchProviders = vi.mocked(api.fetchProviders);
const mockFetchLoginStatus = vi.mocked(api.fetchLoginStatus);

function renderPage() {
  return render(
    <MemoryRouter>
      <AuthProvider>
        <LoginConfirmedPage />
      </AuthProvider>
    </MemoryRouter>
  );
}

describe("LoginConfirmedPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useRealTimers();
    i18n.changeLanguage("en");
    mockFetchProviders.mockResolvedValue({ email: true, google: false, apple: false, github: false });
  });

  it("shows the brand header and no showings content", async () => {
    mockFetchMe.mockRejectedValue(new Error("not auth"));
    mockFetchLoginStatus.mockResolvedValue(false);
    const { container } = renderPage();
    await screen.findByText(/waiting|Sign-in confirmed|logged in/i);
    expect(screen.getByRole("heading", { name: "OV Cinema Linz" })).toBeInTheDocument();
    expect(container.querySelector(".marquee-logo")).toHaveAttribute("src", "/projector-logo.svg");
    // no showings content
    expect(screen.queryByText("Megaplex PlusCity")).toBeNull();
    expect(screen.queryByText("Impressum")).toBeNull();
  });

  it("shows the other-device message when not logged in", async () => {
    mockFetchMe.mockRejectedValue(new Error("not auth"));
    mockFetchLoginStatus.mockResolvedValue(false);
    renderPage();
    expect(await screen.findByText(/Sign-in confirmed/)).toBeInTheDocument();
  });

  it("shows the logged-in message after the mount poll succeeds", async () => {
    vi.useFakeTimers();
    mockFetchMe
      .mockRejectedValueOnce(new Error("not auth"))
      .mockResolvedValueOnce({ id: 1, email: "a@b.com" });
    mockFetchLoginStatus.mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    renderPage();
    await act(async () => {});
    expect(screen.queryByText(/You have been logged in/)).toBeNull();
    await act(async () => { vi.advanceTimersByTime(1000); });
    await act(async () => {});
    await act(async () => {});
    expect(screen.getByText(/You have been logged in/)).toBeInTheDocument();
  });
});
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `npm test -- src/pages/LoginConfirmedPage.test.tsx`
Expected: FAIL — `Cannot find module './LoginConfirmedPage'` / component not exported.

- [ ] **Step 4: Implement `LoginConfirmedPage`**

Create `frontend/src/pages/LoginConfirmedPage.tsx`:

```tsx
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useAuth } from "../hooks/useAuth";

export function LoginConfirmedPage() {
  const { t } = useTranslation();
  const { user, loading, pollLoginStatus } = useAuth();

  useEffect(() => {
    if (user || loading) return;
    let cancelled = false;
    void pollLoginStatus(undefined, 20_000, () => cancelled);
    return () => {
      cancelled = true;
    };
  }, [user, loading, pollLoginStatus]);

  return (
    <>
      <header className="marquee">
        <div className="bulbs"></div>
        <div className="marquee-brand">
          <img className="marquee-logo" src="/projector-logo.svg" alt="" />
          <div className="marquee-text">
            <h1>{t("brand")}</h1>
            <p className="tagline">{t("tagline")}</p>
          </div>
        </div>
        <div className="bulbs"></div>
      </header>
      <main>
        <p className="auth-note">{user ? t("auth.loggedIn") : t("auth.confirmed")}</p>
      </main>
    </>
  );
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `npm test -- src/pages/LoginConfirmedPage.test.tsx`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add frontend/src/pages/LoginConfirmedPage.tsx frontend/src/pages/LoginConfirmedPage.test.tsx frontend/src/locales
git commit -m "feat: add login-confirmed landing page"
```

---

### Task 2: Route `?login=confirmed` to the landing page; strip dead code from Marquee

**Files:**
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/components/Marquee.tsx`
- Modify: `frontend/src/components/Marquee.test.tsx`

**Interfaces:**
- Consumes: `LoginConfirmedPage` (Task 1), `ShowingsPage` (existing).
- Produces: `/` with `?login=confirmed` renders `LoginConfirmedPage`; `Marquee.tsx` no longer imports `useSearchParams`, has no `confirmed` state, no mount poll, no `auth-panel`/`auth-note` banner.

- [ ] **Step 1: Update `App.tsx` to branch on the query param**

Replace the body of `App.tsx`:

```tsx
import { useSearchParams } from "react-router-dom";
import { AuthProvider } from "./hooks/useAuth";
import { ShowingsPage } from "./pages/ShowingsPage";
import { ImpressumPage } from "./pages/ImpressumPage";
import { LoginConfirmedPage } from "./pages/LoginConfirmedPage";

export default function App() {
  const [searchParams] = useSearchParams();
  const confirmed = searchParams.get("login") === "confirmed";
  return (
    <AuthProvider>
      {confirmed ? (
        <LoginConfirmedPage />
      ) : (
        <Routes>
          <Route path="/" element={<ShowingsPage />} />
          <Route path="/impressum" element={<ImpressumPage />} />
        </Routes>
      )}
    </AuthProvider>
  );
}
```

Update the import on the first line to include `Routes`, `Route`, and `useSearchParams`:

```tsx
import { Route, Routes, useSearchParams } from "react-router-dom";
```

- [ ] **Step 2: Remove dead confirmed code from `Marquee.tsx`**

- Remove `useSearchParams` from the `react-router-dom` import (line 2) — it becomes `import { NavLink } from "react-router-dom";`.
- Remove the `confirmed` const (line 14).
- Remove the mount-poll `useEffect` block (lines 16-23).
- Remove the `{confirmed && !user && !loading && (...)}` banner block (lines 66-70).
- Remove the now-unused `useEffect` import from `react` (line 1): `import { useState, type FormEvent } from "react";`.

- [ ] **Step 3: Remove the confirmed-banner test from `Marquee.test.tsx`**

Remove the `it("renders the confirmed banner when ?login=confirmed", ...)` test block. Keep all other auth tests. Also verify `useSearchParams`/`MemoryRouter initialEntries` imports are still used by the remaining tests (the login-panel tests use plain `<MemoryRouter>`).

- [ ] **Step 4: Run the full frontend suite and build**

```bash
npm test
npm run build
```

Expected: all pass (including the new `LoginConfirmedPage` tests); build succeeds.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/App.tsx frontend/src/components/Marquee.tsx frontend/src/components/Marquee.test.tsx
git commit -m "feat: route login=confirmed to landing page; drop dead Marquee banner"
```

---

### Task 3: Verify and deploy

**Files:** none (CI + manual verification)

**Interfaces:**
- Consumes: Tasks 1-2.

- [ ] **Step 1: Run the full frontend suite once more**

```bash
cd frontend && npm test && npm run build
```

- [ ] **Step 2: Push to trigger CI/CD**

```bash
git push origin master
```

Watch: `gh run watch --repo semtexkg/cinema --exit-status`
Expected: test, build, deploy all green.

- [ ] **Step 3: Manual verification**

1. Open https://cinema.k-labs.app on a phone and request a magic link, then click the link on the phone → the phone lands on the brand-only landing page with "Sign-in confirmed — you can close this window." and NO showings content.
2. Open the same link in a desktop tab → it shows "You have been logged in — you can close this window." and the desktop is logged in.

---

## Self-review notes

- **Spec coverage:** brand-only landing page (Task 1), adaptive message (Task 1), routing branch (Task 2), Marquee dead-code removal (Task 2), i18n with exact strings (Task 1), tests (Task 1-2), deploy (Task 3). No backend changes, matching spec.
- **Placeholder scan:** no TBD/TODO; all code is concrete.
- **Type consistency:** `pollLoginStatus(undefined, 20_000, () => cancelled)` signature matches `useAuth.tsx` (`(sendEmail?, maxMs = 15*60*1000, isCancelled?)`); `LoginConfirmedPage` exported as a named export and imported named in `App.tsx`; i18n keys `auth.loggedIn` / `auth.confirmed` consistent across both locales and the page.
