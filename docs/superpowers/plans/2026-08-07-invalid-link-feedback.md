# Invalid Sign-In Link Feedback — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show a minimal landing page explaining that a sign-in link is expired or already used, instead of silently landing on the showings page.

**Architecture:** A new `InvalidLinkPage` renders the brand header (logo + title + tagline) and a single message, with no action button and no poll. `App.tsx` gains a branch for `?error=invalid_token`. Mirrors the existing `LoginConfirmedPage` pattern.

**Tech Stack:** React 19 + Vite + TypeScript, react-router-dom, react-i18next, Vitest + Testing Library.

## Global Constraints

- `?error=invalid_token` renders `InvalidLinkPage` — NOT the showings page.
- The page shows the brand header only: logo (`/projector-logo.svg`, class `marquee-logo`), title (`t("brand")`), tagline (`t("tagline")`) inside the existing `marquee-brand` / `marquee-text` classes. No nav, no login panel, no sidebar, no showings content.
- Message: `<p className="auth-note">{t("auth.invalidLink")}</p>`.
- NO action button, NO login form, NO mount poll (nothing to poll for an invalid token).
- `AuthProvider` wraps all branches in `App.tsx`.
- i18n exact strings:
  - `auth.invalidLink` en: "This sign-in link has expired or was already used. Please request a new one on the device where you want to sign in."
  - `auth.invalidLink` de: "Dieser Anmelde-Link ist abgelaufen oder wurde bereits verwendet. Bitte fordere einen neuen Link auf dem Gerät an, auf dem du dich anmelden möchtest."
- Run frontend tests from `frontend/` with `npm test`; build with `npm run build`.
- No backend changes.

---

### Task 1: i18n key and `InvalidLinkPage` component with tests

**Files:**
- Create: `frontend/src/pages/InvalidLinkPage.tsx`
- Create: `frontend/src/pages/InvalidLinkPage.test.tsx`
- Modify: `frontend/src/locales/en.json`, `frontend/src/locales/de.json`

**Interfaces:**
- Consumes: `useTranslation()`, existing CSS classes `marquee`, `bulbs`, `marquee-brand`, `marquee-logo`, `marquee-text`, `tagline`, `auth-note`.
- Produces: `export function InvalidLinkPage()` — brand header + `auth.invalidLink` message, no effect, no auth context usage.

- [ ] **Step 1: Add the i18n key**

`frontend/src/locales/en.json` — add `invalidLink` inside `auth`, after `loggedIn`:

```json
"waiting": "Check your email — waiting for confirmation…",
"confirmed": "Sign-in confirmed — you can close this window.",
"loggedIn": "You have been logged in — you can close this window.",
"invalidLink": "This sign-in link has expired or was already used. Please request a new one on the device where you want to sign in.",
"signInWith": "Sign in with {{provider}}"
```

`frontend/src/locales/de.json` — add `invalidLink` inside `auth`, after `loggedIn`:

```json
"waiting": "Prüfe deine E-Mails — warte auf Bestätigung…",
"confirmed": "Anmeldung bestätigt — du kannst dieses Fenster schließen.",
"loggedIn": "Du bist angemeldet — du kannst dieses Fenster schließen.",
"invalidLink": "Dieser Anmelde-Link ist abgelaufen oder wurde bereits verwendet. Bitte fordere einen neuen Link auf dem Gerät an, auf dem du dich anmelden möchtest.",
"signInWith": "Anmelden mit {{provider}}"
```

- [ ] **Step 2: Write the failing tests**

Create `frontend/src/pages/InvalidLinkPage.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { describe, it, expect, beforeEach } from "vitest";
import { InvalidLinkPage } from "./InvalidLinkPage";
import i18n from "../i18n";

function renderPage() {
  return render(<InvalidLinkPage />);
}

describe("InvalidLinkPage", () => {
  beforeEach(() => i18n.changeLanguage("en"));

  it("shows the brand header and the invalid-link message", () => {
    const { container } = renderPage();
    expect(screen.getByRole("heading", { name: "OV Cinema Linz" })).toBeInTheDocument();
    expect(container.querySelector(".marquee-logo")).toHaveAttribute("src", "/projector-logo.svg");
    expect(
      screen.getByText(/This sign-in link has expired or was already used/)
    ).toBeInTheDocument();
  });

  it("shows no showings content and no action button", () => {
    const { container } = renderPage();
    expect(screen.queryByText("Megaplex PlusCity")).toBeNull();
    expect(screen.queryByText("Impressum")).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
    expect(container).not.toHaveTextContent(/request a new link/i);
  });
});
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `npm test -- src/pages/InvalidLinkPage.test.tsx`
Expected: FAIL — `Cannot find module './InvalidLinkPage'`.

- [ ] **Step 4: Implement `InvalidLinkPage`**

Create `frontend/src/pages/InvalidLinkPage.tsx`:

```tsx
import { useTranslation } from "react-i18next";

export function InvalidLinkPage() {
  const { t } = useTranslation();
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
        <p className="auth-note">{t("auth.invalidLink")}</p>
      </main>
    </>
  );
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `npm test -- src/pages/InvalidLinkPage.test.tsx`
Expected: PASS (2 tests).

- [ ] **Step 6: Commit**

```bash
git add frontend/src/pages/InvalidLinkPage.tsx frontend/src/pages/InvalidLinkPage.test.tsx frontend/src/locales
git commit -m "feat: add invalid sign-in link page"
```

---

### Task 2: Route `?error=invalid_token` to `InvalidLinkPage`

**Files:**
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/App.test.tsx`

**Interfaces:**
- Consumes: `InvalidLinkPage` (Task 1), `LoginConfirmedPage` (existing).
- Produces: `/` with `?error=invalid_token` renders `InvalidLinkPage`; all other URLs unchanged.

- [ ] **Step 1: Update `App.tsx`**

Replace the body of `App.tsx` (lines 7-21) so the error branch is added:

```tsx
export default function App() {
  const [searchParams] = useSearchParams();
  const confirmed = searchParams.get("login") === "confirmed";
  const invalid = searchParams.get("error") === "invalid_token";
  return (
    <AuthProvider>
      {invalid ? (
        <InvalidLinkPage />
      ) : confirmed ? (
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

Add the import:

```tsx
import { InvalidLinkPage } from "./pages/InvalidLinkPage";
```

- [ ] **Step 2: Add an App test for the error branch**

Add to `frontend/src/App.test.tsx`, inside the `describe("App", ...)` block:

```tsx
it("renders the invalid-link page for ?error=invalid_token", async () => {
  mockFetch({ generatedAt: null, sources: {}, cinemas: [] });
  renderAt("/?error=invalid_token");
  expect(
    await screen.findByText(/This sign-in link has expired or was already used/)
  ).toBeInTheDocument();
  expect(screen.queryByText("Impressum")).toBeNull();
});
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `npm test -- src/App.test.tsx`
Expected: FAIL — the new test cannot find the invalid-link message (the branch doesn't exist yet).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npm test -- src/App.test.tsx`
Expected: PASS (5 tests, including the new one).

- [ ] **Step 5: Run the full frontend suite and build**

```bash
npm test
npm run build
```

Expected: all pass; build succeeds.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/App.tsx frontend/src/App.test.tsx
git commit -m "feat: route error=invalid_token to invalid-link page"
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

1. Request a magic link on https://cinema.k-labs.app and click it once to log in.
2. Click the SAME link again (now used) → you should land on the minimal invalid-link page with "This sign-in link has expired or was already used. Please request a new one on the device where you want to sign in." and no showings content.
3. Open https://cinema.k-labs.app/?error=invalid_token directly → same page.

---

## Self-review notes

- **Spec coverage:** brand-only page with exact message (Task 1), routing branch (Task 2), i18n both locales (Task 1), tests (Tasks 1-2), deploy (Task 3). No button/form/poll, matching the "no link" decision. SSO errors out of scope, matching spec.
- **Placeholder scan:** no TBD/TODO; all code concrete.
- **Type consistency:** `InvalidLinkPage` named export imported named in `App.tsx`; i18n key `auth.invalidLink` consistent across both locales and the page; message substring used in tests matches the exact en string.
