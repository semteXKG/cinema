# Notification Preferences UI — Design Spec

**Date:** 2026-08-09
**Status:** Draft

## Overview

Add a UI-only Preferences page at `/preferences` for logged-in users. The page
lets a user pick, per notification channel (**Email**, **Telegram**), how often
they want to be notified: from **never** to **immediately** to **1–7 days**.
No backend wiring — the controls use local React state and a mock Save button.

## Behavior

| Scenario | Result |
|----------|--------|
| Not logged in | No "Preferences" link in the nav; `/preferences` still renders (no auth guard for now) |
| Logged in, click "Preferences" | Navigates to `/preferences` |
| Change a frequency select | Local state updates (no persistence) |
| Click "Save" | Shows "Saved" confirmation for ~2s, then it disappears |
| Reload page | Defaults return (email = immediately, telegram = never) |

## Component structure

### New: `frontend/src/pages/PreferencesPage.tsx`

Renders:
- `<Marquee />` (consistent with the other pages).
- `<h2>{t("preferences.title")}</h2>`.
- One `.card` per channel:
  - **Email**: title + description + labeled `<select>`.
  - **Telegram**: title + description + labeled `<select>`.
- A **Save** button (`className="auth-submit"`).
- A "Saved" confirmation (`className="pref-saved"`), rendered while `saved`
  is true.

State (local `useState`):
- `emailFreq: NotificationFrequency` default `"immediately"`.
- `telegramFreq: NotificationFrequency` default `"never"`.
- `saved: boolean` — set `true` on Save, cleared after 2s via `setTimeout`
  (cleanup on unmount).

No API calls; uses `useAuth()` only if needed to read the user's email for the
Email channel label (optional).

### New: `frontend/src/types.ts` additions

```ts
export type NotificationFrequency =
  | "never"
  | "immediately"
  | "1" | "2" | "3" | "4" | "5" | "6" | "7";

export const FREQUENCY_OPTIONS: NotificationFrequency[] = [
  "never", "immediately", "1", "2", "3", "4", "5", "6", "7",
];
```

### Modify: `frontend/src/App.tsx`

- Add `<Route path="/preferences" element={<PreferencesPage />} />` to the
  existing `<Routes>` (next to `/impressum`).

### Modify: `frontend/src/components/Marquee.tsx`

- When `user` is non-null, render a `<NavLink to="/preferences">` with label
  `t("nav.preferences")` next to the existing Impressum link.

### Modify: `frontend/src/index.css`

- `.pref-card` / `.pref-field`: label + select layout inside `.card`.
- `.pref-select`: styled like `.auth-input` (`background:var(--bg);
  border:1px solid var(--edge); color:var(--text); border-radius:4px;
  padding:.3rem .5rem; font-size:.8rem`).
- `.pref-saved`: `color:var(--ok); font-size:.8rem; margin-left:.6rem`.

### Modify: `frontend/src/locales/{en,de}.json`

Add a `preferences` block:
- `preferences.title`: "Notification preferences" / "Benachrichtigungseinstellungen".
- `preferences.email`: "Email" / "E-Mail".
- `preferences.emailDesc`: "Get notified by email about new OV showings." /
  "Per E-Mail über neue OV-Vorstellungen informiert werden."
- `preferences.telegram`: "Telegram" / "Telegram".
- `preferences.telegramDesc`: "Get notified on Telegram. Link your account to
  activate." / "Auf Telegram benachrichtigt werden. Verknüpfe deinen Account,
  um es zu aktivieren."
- `preferences.frequency`: "Frequency" / "Häufigkeit".
- `preferences.frequencies.never`: "Never" / "Nie".
- `preferences.frequencies.immediately`: "Immediately" / "Sofort".
- `preferences.frequencies.days`: "{{count}} day" / "{{count}} Tag" with the
  `_plural` form "{{count}} days" / "{{count}} Tage".
- `preferences.save`: "Save" / "Speichern".
- `preferences.saved`: "Saved" / "Gespeichert".
- `nav.preferences`: "Preferences" / "Einstellungen".

Frequency selects iterate `FREQUENCY_OPTIONS` and map each value to its label:
`never`, `immediately`, or `t("preferences.frequencies.days", { count })`.

## Testing

### New: `frontend/src/pages/PreferencesPage.test.tsx`

Render `<MemoryRouter><AuthProvider><PreferencesPage /></AuthProvider></MemoryRouter>`
with `fetch` mocked (for `fetchMe`/`fetchProviders`):
- Renders both channel headings (Email, Telegram) and their frequency selects.
- Defaults: email select shows "Immediately", telegram select shows "Never".
- Selecting a different option updates the select's value.
- Clicking Save shows the "Saved" confirmation.

### Modify: `frontend/src/App.test.tsx`

- Assert the "Preferences" nav link is **absent** when not logged in
  (`/api/auth/me` returns 401).
- Add a test where `/api/auth/me` returns a user and assert the
  "Preferences" link **is** present.

## Out of scope

- Backend: no persistence, no API endpoints, no `checker.rs`/`notify.rs`
  changes, no DB migration. Preferences are never actually honored.
- Linking a Telegram account; the Telegram description mentions it as future
  work but no UI exists for it.
- Auth guard on the route (anyone can visit `/preferences` for now).
