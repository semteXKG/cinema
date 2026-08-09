# Telegram Handle Field on Preferences Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Telegram-handle text input to the Telegram card on `/preferences` (UI-only, local state, mock Save).

**Architecture:** One small frontend change: a `telegramHandle` `useState` in `PreferencesPage`, a text input rendered only for the telegram channel, two i18n keys, one CSS class. No backend, no persistence.

**Tech Stack:** React 19, react-i18next, Vitest + Testing Library.

## Global Constraints

- UI only — no backend, no API, no persistence, no validation (any text accepted).
- The handle input renders ONLY in the Telegram card (Email card unchanged).
- Default handle value: `""`; reload resets it.
- No comments in code.
- Locale keys in both `en.json` and `de.json`.
- Run everything from `frontend/`; tests via `npm test`, build via `npm run build`.

---

### Task 1: Telegram handle input on the preferences page

**Files:**
- Modify: `frontend/src/pages/PreferencesPage.test.tsx`
- Modify: `frontend/src/pages/PreferencesPage.tsx`
- Modify: `frontend/src/index.css`
- Modify: `frontend/src/locales/en.json`
- Modify: `frontend/src/locales/de.json`

**Interfaces:**
- Consumes: existing `useState`, `useTranslation`, the `channels` map in `PreferencesPage.tsx`; locale keys `preferences.*`.
- Produces: a text input labeled "Telegram handle" (aria-label + visible span) with placeholder `@yourhandle`, backed by local state `telegramHandle`; rendered only when the current channel is `"telegram"`.

- [ ] **Step 1: Write the failing tests**

Append to `frontend/src/pages/PreferencesPage.test.tsx` (inside the existing `describe("PreferencesPage", ...)` block; `within` is already imported):

```tsx
it("shows the telegram handle input only in the telegram card", async () => {
  mockAuthFetch();
  renderPage();
  await screen.findByRole("heading", { name: "Notification preferences" });
  const emailCard = screen.getByRole("heading", { name: "Email" }).closest(".pref-card")!;
  const telegramCard = screen.getByRole("heading", { name: "Telegram" }).closest(".pref-card")!;
  expect(within(emailCard).queryByPlaceholderText("@yourhandle")).toBeNull();
  expect(within(telegramCard).getByPlaceholderText("@yourhandle")).toBeInTheDocument();
});

it("updates the telegram handle as the user types", async () => {
  mockAuthFetch();
  renderPage();
  await screen.findByRole("heading", { name: "Notification preferences" });
  const input = screen.getByPlaceholderText("@yourhandle");
  fireEvent.change(input, { target: { value: "@myhandle" } });
  expect(input).toHaveValue("@myhandle");
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npm test`
Expected: FAIL — no element with placeholder `@yourhandle` exists.

- [ ] **Step 3: Add the i18n keys**

`frontend/src/locales/en.json` — under `preferences`, after `frequency`:

```json
  "frequency": "Frequency",
  "telegramHandle": "Telegram handle",
  "telegramHandlePlaceholder": "@yourhandle",
```

`frontend/src/locales/de.json` — under `preferences`, after `frequency`:

```json
  "frequency": "Häufigkeit",
  "telegramHandle": "Telegram-Benutzername",
  "telegramHandlePlaceholder": "@deinname",
```

- [ ] **Step 4: Implement the component**

`frontend/src/pages/PreferencesPage.tsx` — add the state next to the other `useState` calls:

```tsx
  const [telegramHandle, setTelegramHandle] = useState("");
```

Inside the channel map, after the frequency `<label className="pref-field">...` block (i.e. after the closing `</label>` of the select), add:

```tsx
          {c.name === "telegram" && (
            <label className="pref-field">
              <span>{t("preferences.telegramHandle")}</span>
              <input
                className="pref-input"
                type="text"
                placeholder={t("preferences.telegramHandlePlaceholder")}
                value={telegramHandle}
                onChange={(e) => setTelegramHandle(e.target.value)}
                aria-label={t("preferences.telegramHandle")}
              />
            </label>
          )}
```

- [ ] **Step 5: Add the CSS**

Append to `frontend/src/index.css`:

```css
 .pref-input{background:var(--bg);border:1px solid var(--edge);color:var(--text);
  border-radius:4px;padding:.3rem .5rem;font-size:.8rem;width:200px}
 .pref-input::placeholder{color:var(--faint)}
 .pref-input:focus{outline:none;border-color:var(--gold)}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd frontend && npm test`
Expected: PASS — 2 new tests green, full suite green.

- [ ] **Step 7: Typecheck and build**

Run: `cd frontend && npm run build`
Expected: build succeeds.

- [ ] **Step 8: Commit**

```bash
git add frontend/src/pages/PreferencesPage.test.tsx frontend/src/pages/PreferencesPage.tsx frontend/src/index.css frontend/src/locales/en.json frontend/src/locales/de.json
git commit -m "feat: telegram handle field on preferences page"
```
