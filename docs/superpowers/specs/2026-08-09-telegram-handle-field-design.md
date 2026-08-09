# Telegram Handle Field on Preferences — Design Spec

**Date:** 2026-08-09
**Status:** Draft

## Overview

Add a Telegram-handle text input to the **Telegram** channel card on the
`/preferences` page so a user can enter their Telegram username. UI-only, like
the rest of the page: the value lives in local React state and is part of the
mock Save. The Email card is unchanged.

## Behavior

| Action | Result |
|--------|--------|
| Open `/preferences` | Telegram card shows a "Telegram handle" text input (placeholder `@yourhandle`), value empty |
| Type a handle | Local state updates; the field reflects it |
| Click Save | "Saved" confirmation appears (existing mock behavior); the handle stays in the field |
| Reload | Handle resets to empty (no persistence) |

The input is rendered **only** in the Telegram card; the Email card keeps just
its frequency select.

## Component structure

### Modify: `frontend/src/pages/PreferencesPage.tsx`

- Add local state `const [telegramHandle, setTelegramHandle] = useState("");`.
- Inside the channel map, render the handle field only for the telegram
  channel, below the frequency select:
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

### Modify: `frontend/src/index.css`

Add `.pref-input` styled like `.pref-select`:

```css
 .pref-input{background:var(--bg);border:1px solid var(--edge);color:var(--text);
  border-radius:4px;padding:.3rem .5rem;font-size:.8rem;width:200px}
 .pref-input::placeholder{color:var(--faint)}
 .pref-input:focus{outline:none;border-color:var(--gold)}
```

### Modify: `frontend/src/locales/{en,de}.json`

Under `preferences`:
- `telegramHandle`: en `"Telegram handle"`, de `"Telegram-Benutzername"`.
- `telegramHandlePlaceholder`: en `"@yourhandle"`, de `"@deinname"`.

## Testing

### Modify: `frontend/src/pages/PreferencesPage.test.tsx`

- Assert the Telegram card renders the handle input (placeholder
  `@yourhandle`).
- Assert typing into it updates its value.
- Assert the Email card does not render the handle input.

## Out of scope

- Backend: no persistence, no API endpoint, no DB column — the handle is not
  stored or honored yet.
- Validation (the field accepts any text).
- Telegram identity linking / verification.
