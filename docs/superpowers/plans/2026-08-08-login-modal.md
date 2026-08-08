# Login Modal Rework — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the inline login panel with a centered modal that stacks email (top), a separator, then SSO buttons (Google/GitHub), dismissible via overlay, Escape, and a close button.

**Architecture:** A new `LoginModal` component owns the email form, waiting state, and SSO buttons; it reads `useAuth()` internally and takes `{ open, onClose }` props. `Marquee.tsx` keeps only the Sign in/out trigger and renders the modal. New modal CSS classes + an `auth.or` i18n key.

**Tech Stack:** React 19 + Vite + TypeScript, react-i18next, Vitest + Testing Library.

## Global Constraints

- Modal dismisses on: overlay click, Escape key, and a visible × button.
- Modal contents, top to bottom: title (`t("auth.signIn")`), email form (input + submit, with the existing waiting state), separator (`t("auth.or")`), then config-driven SSO buttons (Google, Apple if configured, GitHub) — exactly the current provider logic from `Marquee.tsx:72-86`.
- After email submit the modal stays open showing the waiting state; when `user` flips non-null (poll success) the modal calls `onClose()`.
- `useAuth` API is unchanged (`user`, `providers`, `loginEmail`, `loginSSO`, `logout`, `pollLoginStatus`).
- i18n new key: `auth.or` — en `"or"`, de `"oder"`. All other `auth.*` keys unchanged.
- Landing pages (`LoginConfirmedPage`, `InvalidLinkPage`) and their routes are untouched.
- Run frontend tests from `frontend/` with `npm test`; build with `npm run build`.

---

### Task 1: `LoginModal` component, CSS, and i18n key with tests

**Files:**
- Create: `frontend/src/components/LoginModal.tsx`
- Create: `frontend/src/components/LoginModal.test.tsx`
- Modify: `frontend/src/index.css` (append modal styles)
- Modify: `frontend/src/locales/en.json`, `frontend/src/locales/de.json`

**Interfaces:**
- Consumes: `useAuth()` (`{ user, providers, loginEmail, loginSSO }`), existing `.auth-input`, `.auth-submit`, `.auth-sso`, `.auth-note`, `.auth-btn` classes, existing i18n keys.
- Produces: `export function LoginModal({ open, onClose }: { open: boolean; onClose: () => void })` — renders the dialog when `open`.

- [ ] **Step 1: Add the i18n key**

`frontend/src/locales/en.json` — inside `auth`, after `signInWith`:

```json
"signInWith": "Sign in with {{provider}}",
"or": "or"
```

`frontend/src/locales/de.json` — inside `auth`, after `signInWith`:

```json
"signInWith": "Anmelden mit {{provider}}",
"or": "oder"
```

- [ ] **Step 2: Write the failing tests**

Create `frontend/src/components/LoginModal.test.tsx`:

```tsx
import { render, screen, fireEvent, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { LoginModal } from "./LoginModal";
import { AuthProvider } from "../hooks/useAuth";
import i18n from "../i18n";
import * as api from "../api";

vi.mock("../api");

const mockFetchMe = vi.mocked(api.fetchMe);
const mockFetchProviders = vi.mocked(api.fetchProviders);
const mockSendMagicLink = vi.mocked(api.sendMagicLink);
const mockFetchLoginStatus = vi.mocked(api.fetchLoginStatus);

const onClose = vi.fn();

function renderModal(open = true) {
  return render(
    <MemoryRouter>
      <AuthProvider>
        <LoginModal open={open} onClose={onClose} />
      </AuthProvider>
    </MemoryRouter>
  );
}

describe("LoginModal", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useRealTimers();
    i18n.changeLanguage("en");
    mockFetchMe.mockRejectedValue(new Error("not auth"));
    mockFetchProviders.mockResolvedValue({
      email: true,
      google: true,
      apple: false,
      github: true,
    });
    mockSendMagicLink.mockResolvedValue(undefined);
    mockFetchLoginStatus.mockResolvedValue(false);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("renders email form and config-driven SSO buttons", async () => {
    renderModal();
    await act(async () => {});
    expect(screen.getByPlaceholderText("your@email.com")).toBeInTheDocument();
    expect(screen.getByText("Sign in with Google")).toBeInTheDocument();
    expect(screen.getByText("Sign in with GitHub")).toBeInTheDocument();
    expect(screen.queryByText("Sign in with Apple")).toBeNull();
  });

  it("renders nothing when closed", async () => {
    renderModal(false);
    await act(async () => {});
    expect(screen.queryByPlaceholderText("your@email.com")).toBeNull();
  });

  it("closes on overlay click", async () => {
    renderModal();
    await act(async () => {});
    fireEvent.click(screen.getByTestId("modal-overlay"));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("closes on Escape", async () => {
    renderModal();
    await act(async () => {});
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("closes via the close button", async () => {
    renderModal();
    await act(async () => {});
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("shows the waiting state after email submit", async () => {
    vi.useFakeTimers();
    renderModal();
    await act(async () => {});
    fireEvent.change(screen.getByPlaceholderText("your@email.com"), {
      target: { value: "a@b.com" },
    });
    fireEvent.click(screen.getByText("Send link"));
    expect(screen.getByText(/waiting for confirmation/)).toBeInTheDocument();
  });
});
```

Note: the overlay needs a `data-testid="modal-overlay"` attribute for the
test.

- [ ] **Step 3: Run the tests to verify they fail**

Run: `npm test -- src/components/LoginModal.test.tsx`
Expected: FAIL — `Cannot find module './LoginModal'`.

- [ ] **Step 4: Implement `LoginModal`**

Create `frontend/src/components/LoginModal.tsx`:

```tsx
import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { useAuth } from "../hooks/useAuth";

interface LoginModalProps {
  open: boolean;
  onClose: () => void;
}

export function LoginModal({ open, onClose }: LoginModalProps) {
  const { t } = useTranslation();
  const { user, providers, loginEmail, loginSSO } = useAuth();
  const [emailInput, setEmailInput] = useState("");
  const [sending, setSending] = useState(false);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  useEffect(() => {
    if (open && user) onClose();
  }, [open, user, onClose]);

  if (!open) return null;

  const handleEmailSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!emailInput.trim() || sending) return;
    setSending(true);
    try {
      await loginEmail(emailInput.trim());
    } finally {
      setSending(false);
    }
  };

  return (
    <div className="modal-overlay" data-testid="modal-overlay" onClick={onClose}>
      <div className="modal" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
        <button className="modal-close" aria-label="Close" onClick={onClose}>
          ×
        </button>
        <h2 className="modal-title">{t("auth.signIn")}</h2>
        {providers?.email && (
          <form onSubmit={handleEmailSubmit}>
            <input
              className="auth-input"
              type="email"
              placeholder={t("auth.emailPlaceholder")}
              value={emailInput}
              onChange={(e) => setEmailInput(e.target.value)}
              disabled={sending}
            />
            <button className="auth-submit" type="submit" disabled={sending}>
              {sending ? t("auth.waiting") : t("auth.sendLink")}
            </button>
          </form>
        )}
        <div className="modal-divider">{t("auth.or")}</div>
        {providers?.google && (
          <button className="auth-sso" onClick={() => loginSSO("google")}>
            {t("auth.signInWith", { provider: "Google" })}
          </button>
        )}
        {providers?.apple && (
          <button className="auth-sso" onClick={() => loginSSO("apple")}>
            {t("auth.signInWith", { provider: "Apple" })}
          </button>
        )}
        {providers?.github && (
          <button className="auth-sso" onClick={() => loginSSO("github")}>
            {t("auth.signInWith", { provider: "GitHub" })}
          </button>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 5: Add modal styles to `index.css`**

Append to `frontend/src/index.css`:

```css
 .modal-overlay{position:fixed;inset:0;background:rgba(0,0,0,.55);
  display:flex;align-items:center;justify-content:center;z-index:100;
  padding:1rem}
 .modal{background:var(--panel);border:1px solid var(--edge);border-radius:8px;
  padding:1.2rem 1.4rem;max-width:320px;width:100%;text-align:center;
  position:relative}
 .modal-title{margin:0 0 .8rem;color:var(--gold);font-size:1.1rem}
 .modal-close{position:absolute;top:.4rem;right:.5rem;background:none;
  border:1px solid var(--edge);color:var(--dim);border-radius:4px;
  padding:0 .4rem;font-size:.9rem;cursor:pointer}
 .modal-close:hover{color:var(--gold);border-color:var(--gold)}
 .modal form{display:flex;flex-direction:column;gap:.5rem;align-items:center}
 .modal .auth-input{width:100%;max-width:220px}
 .modal .auth-sso{display:block;width:100%;margin-top:.5rem}
 .modal-divider{margin:.9rem 0;color:var(--dim);font-size:.75rem;
  display:flex;align-items:center;gap:.6rem}
 .modal-divider::before,.modal-divider::after{content:"";flex:1;
  height:1px;background:var(--edge)}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `npm test -- src/components/LoginModal.test.tsx`
Expected: PASS (6 tests).

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/LoginModal.tsx frontend/src/components/LoginModal.test.tsx frontend/src/index.css frontend/src/locales
git commit -m "feat: add login modal with email + SSO options"
```

---

### Task 2: Wire the modal into the Marquee

**Files:**
- Modify: `frontend/src/components/Marquee.tsx`
- Modify: `frontend/src/components/Marquee.test.tsx`

**Interfaces:**
- Consumes: `LoginModal` (Task 1), `useAuth()` (`{ user, loading, logout }`).
- Produces: Marquee keeps the Sign in/out buttons; Sign in opens the modal; the inline `auth-panel` and its email/SSO logic are removed.

- [ ] **Step 1: Rewrite `Marquee.tsx`**

Replace the whole file with:

```tsx
import { useState } from "react";
import { NavLink } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { LanguageSwitcher } from "./LanguageSwitcher";
import { useAuth } from "../hooks/useAuth";
import { LoginModal } from "./LoginModal";

export function Marquee() {
  const { t } = useTranslation();
  const { user, loading, logout } = useAuth();
  const [showLogin, setShowLogin] = useState(false);

  return (
    <header className="marquee">
      <div className="bulbs"></div>
      <div className="marquee-brand">
        <img className="marquee-logo" src="/projector-logo.svg" alt="" />
        <div className="marquee-text">
          <h1>{t("brand")}</h1>
          <p className="tagline">{t("tagline")}</p>
        </div>
      </div>
      <nav className="marqnav">
        <NavLink to="/">{t("nav.home")}</NavLink>
        <NavLink to="/impressum">{t("nav.impressum")}</NavLink>
        <LanguageSwitcher />
        {!loading &&
          (!user ? (
            <button className="auth-btn" onClick={() => setShowLogin(true)}>
              {t("auth.signIn")}
            </button>
          ) : (
            <button className="auth-btn" onClick={logout}>
              {t("auth.signOut")}
            </button>
          ))}
      </nav>
      <LoginModal open={showLogin} onClose={() => setShowLogin(false)} />
      <div className="bulbs"></div>
    </header>
  );
}
```

- [ ] **Step 2: Update `Marquee.test.tsx`**

The existing tests already assert the email input, "Sign in with Google", and
the waiting state appear after clicking "Sign in" — those now come from the
modal and still pass. Adjust only what needs adjusting:

- The "shows login panel with email and Google SSO buttons" test name may
  stay or be renamed; the assertions are unchanged. Verify
  `screen.queryByText("Sign in with Apple")` still returns null (the modal
  renders Apple only when `apple:true`).
- The waiting-state test: after `fireEvent.click(screen.getByText("Sign in"))`
  and submitting, the modal is open and shows `/waiting for confirmation/`.
  This still works. If the overlay intercepts clicks, note the modal's dialog
  stops propagation, so interacting with the form is fine.
- No test changes should be required, but run the suite to confirm.

Run: `npm test -- src/components/Marquee.test.tsx`
Expected: all 5 Marquee tests pass unchanged.

- [ ] **Step 3: Run the full frontend suite and build**

```bash
npm test
npm run build
```

Expected: all pass (including the new LoginModal tests); build succeeds.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/Marquee.tsx frontend/src/components/Marquee.test.tsx
git commit -m "feat: open login modal from the sign-in button"
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

1. Open https://cinema.k-labs.app → click **Sign in** → the modal opens with
   email on top, "or" divider, then Sign in with Google and Sign in with
   GitHub below.
2. Close via overlay click, Escape, and the × button — all three dismiss it.
3. Submit an email → the modal shows the waiting state and stays open.
4. Click "Sign in with GitHub" → authorize → logged in, header flips to
   "Sign out".
5. On a phone, confirm the modal is centered and usable.

---

## Self-review notes

- **Spec coverage:** modal component + email/SSO/divider + waiting state (Task 1), dismissal via overlay/Esc/× (Task 1), Marquee wiring (Task 2), i18n `auth.or` (Task 1), tests (Tasks 1-2), deploy (Task 3). Landing pages untouched.
- **Placeholder scan:** all code concrete; no TBD/TODO.
- **Type consistency:** `LoginModal({ open, onClose })` signature matches between Task 1 definition and Task 2 usage; `useAuth()` keys (`user`, `providers`, `loginEmail`, `loginSSO`) match the existing hook; i18n `auth.or` consistent across locales and the component.
