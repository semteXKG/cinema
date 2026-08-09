# Notification Preferences UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a UI-only `/preferences` page where logged-in users pick, per channel (Email, Telegram), a notification frequency from never → immediately → 1–7 days, with local state and a mock Save.

**Architecture:** React page (`PreferencesPage`) + a new route + a conditional nav link. Frequency values are a shared union type + constant in `types.ts`; labels come from i18n (`en`/`de`) with pluralization for day counts. No backend, no persistence, no API calls. Styling reuses the existing `.card` / `.auth-submit` classes plus a few new `.pref-*` rules.

**Tech Stack:** React 19, react-router-dom, react-i18next (i18next v26), Vite, Vitest + Testing Library (jsdom).

## Global Constraints

- UI only — NO backend, NO API endpoints, NO persistence, NO DB migrations.
- No comments in code.
- Follow existing patterns: named exports, `useTranslation`, locale keys in both `en.json` and `de.json`.
- Frequency option order is fixed: `never`, `immediately`, `1`, `2`, `3`, `4`, `5`, `6`, `7`.
- Defaults: email = `immediately`, telegram = `never`.
- The "Preferences" nav link renders ONLY when `useAuth().user` is non-null.
- Run everything from `frontend/`; tests via `npm test`, typecheck/build via `npm run build`.

---

### Task 1: Frequency types and i18n keys

**Files:**
- Modify: `frontend/src/types.ts` (append at end)
- Modify: `frontend/src/locales/en.json`
- Modify: `frontend/src/locales/de.json`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `export type NotificationFrequency = "never" | "immediately" | "1" | "2" | "3" | "4" | "5" | "6" | "7"`
  - `export const FREQUENCY_OPTIONS: NotificationFrequency[]` (in that exact order)
  - Locale keys: `nav.preferences`, and the `preferences` block (`title`, `email`, `emailDesc`, `telegram`, `telegramDesc`, `frequency`, `frequencies.never`, `frequencies.immediately`, `frequencies.days_one`, `frequencies.days_other`, `save`, `saved`).

- [ ] **Step 1: Add the types**

Append to `frontend/src/types.ts`:

```ts
export type NotificationFrequency =
  | "never"
  | "immediately"
  | "1"
  | "2"
  | "3"
  | "4"
  | "5"
  | "6"
  | "7";

export const FREQUENCY_OPTIONS: NotificationFrequency[] = [
  "never", "immediately", "1", "2", "3", "4", "5", "6", "7",
];
```

- [ ] **Step 2: Add English locale keys**

In `frontend/src/locales/en.json`, add `"preferences": "Preferences"` under the `nav` object, and a new top-level `preferences` object (after `impressum`):

```json
"preferences": {
  "title": "Notification preferences",
  "email": "Email",
  "emailDesc": "Get notified by email about new OV showings.",
  "telegram": "Telegram",
  "telegramDesc": "Get notified on Telegram. Link your account to activate.",
  "frequency": "Frequency",
  "frequencies": {
    "never": "Never",
    "immediately": "Immediately",
    "days_one": "{{count}} day",
    "days_other": "{{count}} days"
  },
  "save": "Save",
  "saved": "Saved"
}
```

- [ ] **Step 3: Add German locale keys**

In `frontend/src/locales/de.json`, add `"preferences": "Einstellungen"` under the `nav` object, and a new top-level `preferences` object:

```json
"preferences": {
  "title": "Benachrichtigungseinstellungen",
  "email": "E-Mail",
  "emailDesc": "Per E-Mail über neue OV-Vorstellungen informiert werden.",
  "telegram": "Telegram",
  "telegramDesc": "Auf Telegram benachrichtigt werden. Verknüpfe deinen Account, um es zu aktivieren.",
  "frequency": "Häufigkeit",
  "frequencies": {
    "never": "Nie",
    "immediately": "Sofort",
    "days_one": "{{count}} Tag",
    "days_other": "{{count}} Tage"
  },
  "save": "Speichern",
  "saved": "Gespeichert"
}
```

- [ ] **Step 4: Verify JSON validity and no type errors**

Run: `cd frontend && npm run build`
Expected: build succeeds (runs `tsc --noEmit` + Vite build; would fail on malformed JSON or unused/broken types).

- [ ] **Step 5: Run the existing test suite**

Run: `cd frontend && npm test`
Expected: all existing tests pass (regression guard — keys added are not yet referenced).

- [ ] **Step 6: Commit**

```bash
git add frontend/src/types.ts frontend/src/locales/en.json frontend/src/locales/de.json
git commit -m "feat: notification frequency types and i18n keys"
```

---

### Task 2: PreferencesPage component with tests (TDD)

**Files:**
- Create: `frontend/src/pages/PreferencesPage.test.tsx`
- Create: `frontend/src/pages/PreferencesPage.tsx`
- Modify: `frontend/src/index.css` (append at end)

**Interfaces:**
- Consumes: `NotificationFrequency` + `FREQUENCY_OPTIONS` from `../types`; `useTranslation()`; locale keys from Task 1.
- Produces: `export function PreferencesPage()` — named export, no props, renders `<Marquee />`, two `.card` channel sections each with a labeled `<select>`, and a Save button + "Saved" confirmation. Uses only local `useState`; no `useAuth()`.

- [ ] **Step 1: Write the failing test**

Create `frontend/src/pages/PreferencesPage.test.tsx`:

```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import i18n from "../i18n";
import { AuthProvider } from "../hooks/useAuth";
import { PreferencesPage } from "./PreferencesPage";

function mockAuthFetch() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.startsWith("/api/auth/me")) {
        return { ok: true, json: async () => ({ id: 1, email: "a@b.c" }) };
      }
      if (url.startsWith("/api/auth/providers")) {
        return { ok: true, json: async () => ({ email: true, google: true, github: true }) };
      }
      return { ok: false, status: 404 };
    })
  );
}

function renderPage() {
  return render(
    <MemoryRouter>
      <AuthProvider>
        <PreferencesPage />
      </AuthProvider>
    </MemoryRouter>
  );
}

afterEach(() => vi.unstubAllGlobals());
beforeEach(() => i18n.changeLanguage("en"));

describe("PreferencesPage", () => {
  it("renders both channels with default frequencies", async () => {
    mockAuthFetch();
    renderPage();
    expect(
      await screen.findByRole("heading", { name: "Notification preferences" })
    ).toBeInTheDocument();
    const email = screen.getByLabelText("Email");
    const telegram = screen.getByLabelText("Telegram");
    expect(email).toHaveValue("immediately");
    expect(telegram).toHaveValue("never");
  });

  it("updates a channel frequency when the select changes", async () => {
    mockAuthFetch();
    renderPage();
    await screen.findByRole("heading", { name: "Notification preferences" });
    const email = screen.getByLabelText("Email");
    fireEvent.change(email, { target: { value: "3" } });
    expect(email).toHaveValue("3");
    expect(screen.getByRole("option", { name: "3 days" })).toBeInTheDocument();
  });

  it("shows a saved confirmation when Save is clicked", async () => {
    mockAuthFetch();
    renderPage();
    await screen.findByRole("heading", { name: "Notification preferences" });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Saved")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd frontend && npm test`
Expected: FAIL — module `./PreferencesPage` has no exported member `PreferencesPage`.

- [ ] **Step 3: Implement the component**

Create `frontend/src/pages/PreferencesPage.tsx`:

```tsx
import { useEffect, useState } from "react";
import { useTranslation, type TFunction } from "react-i18next";
import { Marquee } from "../components/Marquee";
import { FREQUENCY_OPTIONS, type NotificationFrequency } from "../types";

function frequencyLabel(t: TFunction, value: NotificationFrequency): string {
  if (value === "never") return t("preferences.frequencies.never");
  if (value === "immediately") return t("preferences.frequencies.immediately");
  return t("preferences.frequencies.days", { count: Number(value) });
}

export function PreferencesPage() {
  const { t } = useTranslation();
  const [emailFreq, setEmailFreq] = useState<NotificationFrequency>("immediately");
  const [telegramFreq, setTelegramFreq] = useState<NotificationFrequency>("never");
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    if (!saved) return;
    const id = setTimeout(() => setSaved(false), 2000);
    return () => clearTimeout(id);
  }, [saved]);

  const channels: Array<{
    name: "email" | "telegram";
    freq: NotificationFrequency;
    onChange: (v: NotificationFrequency) => void;
  }> = [
    { name: "email", freq: emailFreq, onChange: setEmailFreq },
    { name: "telegram", freq: telegramFreq, onChange: setTelegramFreq },
  ];

  return (
    <div className="preferences">
      <Marquee />
      <h2>{t("preferences.title")}</h2>
      {channels.map((c) => (
        <div className="card pref-card" key={c.name}>
          <h3>{t(`preferences.${c.name}`)}</h3>
          <p className="pref-desc">{t(`preferences.${c.name}Desc`)}</p>
          <label className="pref-field">
            <span>{t("preferences.frequency")}</span>
            <select
              className="pref-select"
              aria-label={t(`preferences.${c.name}`)}
              value={c.freq}
              onChange={(e) => c.onChange(e.target.value as NotificationFrequency)}
            >
              {FREQUENCY_OPTIONS.map((v) => (
                <option key={v} value={v}>
                  {frequencyLabel(t, v)}
                </option>
              ))}
            </select>
          </label>
        </div>
      ))}
      <div className="pref-actions">
        <button className="auth-submit" onClick={() => setSaved(true)}>
          {t("preferences.save")}
        </button>
        {saved && <span className="pref-saved">{t("preferences.saved")}</span>}
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Add the CSS**

Append to `frontend/src/index.css`:

```css
 .pref-card h3{color:var(--gold);letter-spacing:.1em;text-transform:uppercase;
  font-size:.85rem;margin:0 0 .3rem}
 .pref-desc{color:var(--dim);font-size:.8rem;margin:0 0 .7rem;max-width:60ch}
 .pref-field{display:flex;align-items:center;gap:.6rem;font-size:.85rem}
 .pref-field span{color:var(--text)}
 .pref-select{background:var(--bg);border:1px solid var(--edge);color:var(--text);
  border-radius:4px;padding:.3rem .5rem;font-size:.8rem;cursor:pointer}
 .pref-select:focus{outline:none;border-color:var(--gold)}
 .pref-actions{display:flex;align-items:center;gap:.6rem;margin-top:1rem}
 .pref-saved{color:var(--ok);font-size:.8rem}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cd frontend && npm test`
Expected: PASS — all three `PreferencesPage` tests green (plus the existing suite).

- [ ] **Step 6: Typecheck and build**

Run: `cd frontend && npm run build`
Expected: build succeeds.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/pages/PreferencesPage.tsx frontend/src/pages/PreferencesPage.test.tsx frontend/src/index.css
git commit -m "feat: notification preferences page with mock save"
```

---

### Task 3: Route and conditional nav link with tests (TDD)

**Files:**
- Modify: `frontend/src/App.test.tsx`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/components/Marquee.tsx`

**Interfaces:**
- Consumes: `PreferencesPage` from `./pages/PreferencesPage`; `useAuth().user`; locale key `nav.preferences` from Task 1.
- Produces: the `/preferences` route; a `NavLink to="/preferences"` inside `.marqnav` rendered only when logged in.

- [ ] **Step 1: Write the failing tests**

Add a helper and tests to `frontend/src/App.test.tsx`. First extend the `mockFetch` helper to accept an optional authed user, then add the new tests inside the existing `describe("App", ...)` block:

```tsx
function mockFetch(body: unknown, authed = false) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.startsWith("/api/auth/me")) {
        return authed
          ? { ok: true, json: async () => ({ id: 1, email: "a@b.c" }) }
          : { ok: false, status: 401 };
      }
      if (url.startsWith("/api/auth/providers")) {
        return { ok: true, json: async () => ({ email: true, google: true, github: true }) };
      }
      if (url.startsWith("/api/auth")) return { ok: false, status: 401 };
      return { ok: true, json: async () => body };
    })
  );
}
```

Tests (add inside `describe("App", ...)`):

```tsx
it("hides the Preferences link when logged out", async () => {
  mockFetch({ generatedAt: null, sources: {}, cinemas: [] });
  renderAt("/");
  await screen.findByRole("button", { name: "Sign in" });
  expect(screen.queryByRole("link", { name: "Preferences" })).toBeNull();
});

it("shows the Preferences link and page when logged in", async () => {
  mockFetch({ generatedAt: null, sources: {}, cinemas: [] }, true);
  renderAt("/");
  const link = await screen.findByRole("link", { name: "Preferences" });
  expect(link).toHaveAttribute("href", "/preferences");
  fireEvent.click(link);
  expect(
    await screen.findByRole("heading", { name: "Notification preferences" })
  ).toBeInTheDocument();
});
```

Update the import at the top of `App.test.tsx` to include `fireEvent`:

```tsx
import { render, screen, fireEvent } from "@testing-library/react";
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npm test`
Expected: FAIL — the two new tests fail (no "Preferences" link exists yet).

- [ ] **Step 3: Add the route**

In `frontend/src/App.tsx`, import `PreferencesPage` and add the route next to `/impressum`:

```tsx
import { PreferencesPage } from "./pages/PreferencesPage";
...
<Route path="/impressum" element={<ImpressumPage />} />
<Route path="/preferences" element={<PreferencesPage />} />
```

- [ ] **Step 4: Add the conditional nav link**

In `frontend/src/components/Marquee.tsx`, inside `.marqnav` (after the Impressum `NavLink`, before `<LanguageSwitcher />`), add:

```tsx
{user && <NavLink to="/preferences">{t("nav.preferences")}</NavLink>}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd frontend && npm test`
Expected: PASS — all tests green, including the two new ones.

- [ ] **Step 6: Typecheck and build**

Run: `cd frontend && npm run build`
Expected: build succeeds.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/App.tsx frontend/src/App.test.tsx frontend/src/components/Marquee.tsx
git commit -m "feat: preferences route and logged-in nav link"
```
