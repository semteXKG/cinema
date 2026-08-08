# Login Modal Frosted Glass + Official SSO Buttons — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the login modal a frosted-glass panel and replace the Google/GitHub buttons with official white buttons carrying the provider logo SVGs.

**Architecture:** CSS-only change for the frosted panel; two inline SVG icon components and a `.auth-provider` button style in the LoginModal. Runs after the Apple SSO removal (`.auth-sso` is already gone).

**Tech Stack:** React 19 + Vite + TypeScript, CSS, Vitest + Testing Library.

## Global Constraints

- `.modal` uses `background:rgba(28,22,17,.72); backdrop-filter:blur(10px) saturate(1.2); -webkit-backdrop-filter:blur(10px) saturate(1.2); border:1px solid rgba(232,179,77,.35)`.
- `.auth-provider` = white (`#fff`), text `#3c4043`, border `#d0d4d9`, hover `#f8f9fa`/`#b8bcc1`, logo SVG `1.1rem`.
- GoogleIcon: 4-color G, `viewBox="0 0 48 48"`, paths `#EA4335`/`#4285F4`/`#FBBC05`/`#34A853`, `aria-hidden`.
- GitHubIcon: octocat, `viewBox="0 0 16 16"`, fill `#181717`, `aria-hidden`.
- Existing `auth.signInWith` i18n reused; no new i18n keys.
- Apple is already removed (preceding plan); no Apple button.
- Run from `frontend/`: `npm test` and `npm run build`.

---

### Task 1: Frosted modal + official SSO buttons

**Files:**
- Modify: `frontend/src/index.css`, `frontend/src/components/LoginModal.tsx`, `frontend/src/components/LoginModal.test.tsx`

**Interfaces:**
- Consumes: `loginSSO(provider)` from `useAuth`; `auth.signInWith` i18n.
- Produces: `.modal` frosted style; `.auth-provider` style; `GoogleIcon`/`GitHubIcon` inline components; Google/GitHub buttons as `.auth-provider`.

- [ ] **Step 1: Update `.modal` in `frontend/src/index.css`**

Replace the `.modal{...}` rule (currently `background:var(--panel); border:1px solid var(--edge); ...`) with:

```css
.modal{background:rgba(28,22,17,.72);backdrop-filter:blur(10px) saturate(1.2);
  -webkit-backdrop-filter:blur(10px) saturate(1.2);
  border:1px solid rgba(232,179,77,.35);border-radius:8px;
  padding:1.2rem 1.4rem;max-width:320px;width:100%;text-align:center;
  position:relative}
```

- [ ] **Step 2: Add `.auth-provider` styles to `frontend/src/index.css`**

Append near the modal styles:

```css
.auth-provider{display:flex;align-items:center;justify-content:center;
  gap:.6rem;width:100%;margin-top:.5rem;background:#fff;color:#3c4043;
  border:1px solid #d0d4d9;border-radius:6px;padding:.5rem .8rem;
  font-size:.85rem;font-weight:500;cursor:pointer}
.auth-provider:hover{background:#f8f9fa;border-color:#b8bcc1}
.auth-provider svg{width:1.1rem;height:1.1rem;flex:0 0 auto}
```

- [ ] **Step 3: Add the icon components and update the buttons in `LoginModal.tsx`**

Add two small components above `LoginModal`:

```tsx
function GoogleIcon() {
  return (
    <svg viewBox="0 0 48 48" aria-hidden="true">
      <path fill="#EA4335" d="M24 9.5c3.54 0 6.71 1.22 9.21 3.6l6.85-6.85C35.9 2.38 30.47 0 24 0 14.62 0 6.51 5.38 2.56 13.22l7.98 6.19C12.43 13.72 17.74 9.5 24 9.5z"/>
      <path fill="#4285F4" d="M46.98 24.55c0-1.57-.15-3.09-.38-4.55H24v9.02h12.94c-.58 2.96-2.26 5.48-4.78 7.18l7.73 6c4.51-4.18 7.09-10.36 7.09-17.65z"/>
      <path fill="#FBBC05" d="M10.53 28.59c-.48-1.45-.76-2.99-.76-4.59s.27-3.14.76-4.59l-7.98-6.19C.92 16.46 0 20.12 0 24c0 3.88.92 7.54 2.56 10.78l7.97-6.19z"/>
      <path fill="#34A853" d="M24 48c6.48 0 11.93-2.13 15.89-5.81l-7.73-6c-2.15 1.45-4.92 2.3-8.16 2.3-6.26 0-11.57-4.22-13.47-9.91l-7.98 6.19C6.51 42.62 14.62 48 24 48z"/>
    </svg>
  );
}

function GitHubIcon() {
  return (
    <svg viewBox="0 0 16 16" fill="#181717" aria-hidden="true">
      <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8z"/>
    </svg>
  );
}
```

Replace the Google and GitHub button blocks with `.auth-provider` markup
including the icons:

```tsx
{providers?.google && (
  <button className="auth-provider" onClick={() => loginSSO("google")}>
    <GoogleIcon />
    <span>{t("auth.signInWith", { provider: "Google" })}</span>
  </button>
)}
{providers?.github && (
  <button className="auth-provider" onClick={() => loginSSO("github")}>
    <GitHubIcon />
    <span>{t("auth.signInWith", { provider: "GitHub" })}</span>
  </button>
)}
```

- [ ] **Step 4: Update `LoginModal.test.tsx`**

The existing text-based queries (`getByText("Sign in with Google")`) still
work. Add an assertion that the provider buttons render an SVG:

```tsx
it("renders provider logos in the SSO buttons", async () => {
  renderModal();
  await act(async () => {});
  const google = screen.getByText("Sign in with Google").closest("button");
  const github = screen.getByText("Sign in with GitHub").closest("button");
  expect(google?.querySelector("svg")).not.toBeNull();
  expect(github?.querySelector("svg")).not.toBeNull();
});
```

- [ ] **Step 5: Run the focused test, then the full suite**

```bash
npm test -- src/components/LoginModal.test.tsx
npm test
npm run build
```

Expected: all pass; build succeeds.

- [ ] **Step 6: Commit**

```bash
git add frontend/src
git commit -m "feat: frosted login modal with official SSO buttons"
```

---

### Task 2: Verify and deploy

**Files:** none

- [ ] **Step 1: Run the full frontend suite once more**

```bash
cd frontend && npm test && npm run build
```

- [ ] **Step 2: Push and watch CI**

```bash
git push origin master
gh run watch --repo semtexkg/cinema --exit-status
```

Expected: test, build, deploy all green.

- [ ] **Step 3: Manual verification**

1. Open https://cinema.k-labs.app → Sign in → modal shows the frosted panel
   with the page blurring behind it.
2. The Google and GitHub buttons are white with their logos.
3. Clicking Sign in with GitHub still logs in.

---

## Self-review notes

- **Spec coverage:** frosted modal (Task 1 Step 1), `.auth-provider` + icons + buttons (Task 1 Steps 2-3), tests (Task 1 Step 4), deploy (Task 2).
- **Placeholder scan:** no TBD/TODO; SVG paths are concrete.
- **Type consistency:** `GoogleIcon`/`GitHubIcon` referenced in the buttons where defined; `.auth-provider` used by both; existing i18n key reused.
