# Rule sentence-builder UI — design

**Date:** 2026-08-20
**Status:** Approved
**Spec author:** opencode (brainstormed with user)

## Problem

The current rule editor on the Preferences page is a flat row of four dropdowns
plus a title input and feature chips — a "config form" pattern that is
intimidating for non-tech-savvy users. There is no guidance on what each field
means, no onboarding, and the layout wraps unpredictably on mobile.

## Goal

Replace the row-of-form-controls rule editor with a **natural-language sentence
builder** that reads like a sentence the user fills in, plus a **template picker**
as the "+ Regel hinzufügen" entry point so users can start from a sensible
preset instead of a blank slate. Both paths (template / new) land in the same
editable sentence.

Keep the existing data model and API (already carries `cinemaId`, `features`,
`titleSubstring`, `frequency`, `channels`). This is a frontend-only change.

## Decisions (from brainstorming)

- **Concept:** combine A (natural-language sentence) + C (preset templates).
  "+ Regel hinzufügen" first asks "Aus Vorlage erstellen" or "Neue Regel
  starten"; both paths produce an editable sentence.
- **Rule ordering:** up/down arrow buttons on each rule card, touch-friendly
  (≥40px tap targets). The backend's first-match-wins semantics are
  communicated by reading top-to-bottom; the arrows let users reorder.
- **Feature selection:** a "+ Feature" pill opens a popover with all 9 features
  as toggle pills (selected = gold ✓). Selected features appear inline in the
  sentence as pills with an ✕ to remove.
- **Defaults for "Neue Regel starten":** Beliebiges Kino, keine Features, kein
  Titel, alle 3 Tage, beide Kanäle.
- **Templates (hardcoded, 3):**
  1. "Alle OV-Vorstellungen sofort per Telegram" — Beliebiges Kino, [OV], sofort,
     [telegram]
  2. "Wöchentlicher Digest per Email" — Beliebiges Kino, [] (alle Features),
     alle 7 Tage, [email]
  3. "Sofort-Alarm für alles, beide Kanäle" — Beliebiges Kino, [], sofort,
     [email, telegram]
- **Telegram-unverified warning** (from the prior channel-per-rule task) stays
  on rules that pick Telegram/Both when the account isn't verified.

## Design

### 1. Component structure

New sub-components under `frontend/src/components/`, each with one clear
responsibility:

- **`RuleSentence.tsx`** — renders one rule as the editable sentence. Owns the
  pill interactions: cinema dropdown, feature pills (+ ✕ remove), "+ Feature"
  popover, title input, frequency dropdown, channel toggle pills, remove button,
  up/down reorder buttons. Calls back to `PreferencesPage` on every change
  (`onChange(patch)`, `onRemove()`, `onMoveUp()`, `onMoveDown()`).
- **`AddRuleChoice.tsx`** — the "+ Regel hinzufügen" entry. Shows two cards
  ("Aus Vorlage erstellen" / "Neue Regel starten"). On "Aus Vorlage" reveals a
  list of the 3 templates; picking one calls `onAdd(templateRule)`. On "Neue
  Regel" calls `onAdd(defaultRule)`. Exports the `DEFAULT_RULE` and
  `TEMPLATES` constants.
- **`FeaturePopover.tsx`** — the feature picker popover. Renders a list of all
  9 `FEATURES` as toggle pills; selected ones are gold ✓. Calls `onToggle(f)`.
  Closes on outside-click or Esc.
- **`PillDropdown.tsx`** — a reusable ▾ pill that opens a dropdown list of
  options. Used for cinema and frequency. Props: `value`, `options`, `onChange`,
  `ariaLabel`. Closes on outside-click or Esc.

`PreferencesPage.tsx` keeps the page container + state management
(`rules`, `addRule(rule)`, `removeRule(i)`, `updateRule(i, patch)`,
`moveRule(i, dir)`). The old `.rule-row`/`.chip` markup is replaced by
`<RuleSentence>`.

### 2. The sentence interaction

Each rule renders as a sentence in a `.sentence` container. Inline elements:

- Cinema: a `PillDropdown` (▾ Beliebiges Kino / Cineplexx Linz / Megaplex
  PlusCity).
- Features: when none selected, the sentence reads "(beliebig)". When some are
  selected, they appear as gold pills with ✕ (click ✕ to remove). A dashed
  "+ Feature" pill opens `FeaturePopover`.
- Title: a `Titel enthält ___` text input (empty = beliebiger Titel). Always
  visible.
- Frequency: a `PillDropdown` (▾ sofort / 1 Tag / … / 7 Tage / Nie).
- Channels: two toggle pills, Email and Telegram. Both on = "Beide". Matches the
  existing `channel` field semantics (`both` ↔ both on). A channel pill is
  **disabled** (can't be turned off) when it is the only one enabled, so a rule
  always has at least one channel — matches the backend's non-empty
  `channels` validation and prevents the user from saving an inert rule.
- Frequency "Nie" (Never): when selected, the rule means "skip this showing",
  so the "über [channel]" part is hidden (nothing is sent). The sentence ends
  at "…und schicke es Nie." with no channel pills rendered. The channel value
  is still sent on save (defaults to "both") so the backend `channels` array is
  non-empty, but routing skips it because `frequency == "never"`.
- Remove: a 🗑 button at the sentence end.
- Reorder: ↑ / ↓ buttons next to remove; disabled at the top/bottom of the list.

German sentence skeleton (the connecting words are static text, only the pills
are interactive):

> Benachrichtige mich, wenn **[▾ Beliebiges Kino]** einen Film zeigt **mit
> [OV ✕] [IMAX ✕] [+ Feature]**, **[Titel enthält ___]**, und schicke es
> **[▾ sofort]** über **[✓ Email] [✓ Telegram]**. **[🗑] [↑↓]**

When no features are selected, "mit [OV ✕] …" collapses to "(beliebig)" and the
"+ Feature" pill stays available.

### 3. Add-rule entry + template picker

Clicking "+ Regel hinzufügen" opens `AddRuleChoice` inline (or as a small sheet
below the button). Two cards:

- **Aus Vorlage erstellen** — on click, reveals the 3 template cards. Each shows
  a name and a one-line summary of its values. Clicking a template calls
  `addRule(templateRule)` and closes the picker.
- **Neue Regel starten** — calls `addRule(DEFAULT_RULE)` and closes.

After adding, the new rule appears at the bottom of the rule list as a sentence.

### 4. Ordering (up/down arrows)

Each `RuleSentence` has ↑ and ↓ buttons. ↑ is disabled when the rule is first,
↓ when last. Clicking dispatches `moveRule(i, -1)` / `moveRule(i, +1)`, which
swaps positions in the `rules` state array (and re-numbers `position`). The
list order is the match order; a short helper text under the rules title
already says "erste passende Regel gewinnt" (carried over from the existing
`rulesDesc` copy, lightly updated).

### 5. CSS

New classes in `frontend/src/index.css`, matching the existing gold-on-dark
cinema theme. Replaces the `.rule-row`/`.chip`/`.rule-remove` classes added in
the prior task:

- `.sentence` — `font-size:.9rem; line-height:2.4; color:var(--text)`.
- `.pill` — base pill: `display:inline-block; border-radius:6px;
  padding:.15rem .6rem; border:1px solid var(--edge); background:var(--panel);
  color:var(--dim); cursor:pointer; transition:…`.
- `.pill-on` — selected pill: `background:rgba(232,179,77,.18);
  border-color:var(--gold); color:var(--gold-bright)`.
- `.pill-drop` — the ▾ dropdown pill variant (same base + a ▾ caret).
- `.pill-remove` — the ✕ on a selected feature pill (small, sits inside).
- `.pill-add` — the dashed "+ Feature" pill.
- `.popover` — `position:absolute; background:var(--panel);
  border:1px solid var(--gold); border-radius:8px; padding:.6rem;
  box-shadow:0 8px 24px rgba(0,0,0,.6); z-index:10;` with a flex-wrap pill list.
- `.rule-actions` — the row holding 🗑 and ↑↓, `margin-left:auto`.
- Touch targets: all interactive pills/buttons `min-height:40px;
  min-width:40px;` and `.sentence` `line-height` keeps rows tappable.
- Responsive: at `max-width:560px`, the sentence wraps between phrases; the
  popover becomes full-width minus padding; `.rule-actions` wraps below.

### 6. Data model & API

No backend change. `NotificationRule` (frontend type) already has `channel:
"email" | "telegram" | "both"` and the API conversion (`channelToChannels`/
`channelsToChannel` in `api/preferences.ts`) from the prior task stays as-is.
The `AddRuleChoice` constants build `NotificationRule` values directly.

### 7. Tests

- **`RuleSentence.test.tsx`** — renders a rule; toggling Email pill flips
  channel off; opening the feature popover and clicking a feature adds it
  (pill appears); clicking ✕ on a feature removes it; typing in the title
  input updates `titleSubstring`; the cinema dropdown picks a cinema;
  the frequency dropdown picks a frequency; the "Nie" frequency hides the
  channel pills; remove button calls `onRemove`;
  up/down buttons call `onMoveUp`/`onMoveDown`; top rule's ↑ is disabled.
- **`AddRuleChoice.test.tsx`** — "Neue Regel starten" calls `onAdd` with
  `DEFAULT_RULE` (assert: Beliebig, no features, frequency "3", channel
  "both"); "Aus Vorlage erstellen" reveals 3 templates; picking the OV
  template calls `onAdd` with `[OV], sofort, telegram`.
- **`FeaturePopover.test.tsx`** — renders all 9 features; selected ones are
  gold; clicking a feature calls `onToggle`; outside-click closes.
- **`PreferencesPage.test.tsx`** — updated: clicking "+ Regel hinzufügen" shows
  the two-card choice; the existing channel-select + save test is rewritten to
  drive the sentence (toggle Telegram pill, save, assert `channels` array on
  the wire). The load-error test stays.

### 8. i18n

Add to `preferences` in both `locales/en.json` and `locales/de.json`:
- `addRuleFromTemplate` ("From a template" / "Aus Vorlage erstellen")
- `addRuleNew` ("Start new" / "Neue Regel starten")
- `addRuleNewDesc`, `addRuleFromTemplateDesc` (the one-line subtitles)
- `templateOvTelegram`, `templateDigestEmail`, `templateInstantAll` (names +
  summaries)
- `sentencePrefix` ("Benachrichtige mich, wenn" / "Notify me when"),
  `sentenceShows` ("einen Film zeigt" / "shows a movie"),
  `sentenceWith` ("mit" / "with"), `sentenceAny` ("(beliebig)" / "(any)"),
  `sentenceTitleContains` ("Titel enthält" / "title contains"),
  `sentenceSend` ("und schicke es" / "and send it"),
  `sentenceOver` ("über" / "via"),
  `addFeature` ("+ Feature" / "+ Feature")
- `rulesDesc` lightly updated to mention up/down for reordering.

### 9. Out of scope

- No backend changes (DB, API, routing, batches unchanged).
- No change to the Telegram handle/verification card above the rules.
- No drag-and-drop reordering (up/down arrows only).
- No user-defined/custom templates (hardcoded set of 3).
- No change to the feature vocabulary or frequency options.
