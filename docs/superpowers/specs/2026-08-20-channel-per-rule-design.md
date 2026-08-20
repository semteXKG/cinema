# Channel-per-rule notifications

**Date:** 2026-08-20
**Status:** Approved
**Spec author:** opencode (brainstormed with user)

## Problem

The Preferences page has two issues:

1. **Broken/ugly rule UI.** `index.css` defines no styles for `.rule-row`,
   `.rule-features`, `.chip`, `.chip-on`, or `.mock-button`, so every button in
   a rule card renders as an unstyled default — the row wraps unpredictably and
   looks broken.
2. **Wrong conceptual model.** The "notify via email / telegram" toggles at the
   top of the page are global per-user switches. The user wants the channel
   choice to be part of each rule, so a rule reads *"If X (cinema/features/title),
   then notify in batch over Email / Telegram / Both"* and different rules can
   target different channels.

## Goal

- Drop the global `email_enabled`/`telegram_enabled` toggles (DB + UI).
- Move the notification channel onto each rule.
- Keep the per-user Telegram handle + verification flow (needed for any telegram
  routing); just drop the enable *toggle*.
- Fix the rule card styling and make it responsive.

## Non-goals

- No change to `verify.rs`, `send.rs`, `schedule.rs`, the `notification_batch`
  table schema, digest scheduling, or the `/api/telegram/webhook` flow.
- No new channels beyond email/telegram.

## Decisions (from brainstorming)

- **Channel selector UX:** a 3-option `<select>` (Email / Telegram / Both) per
  rule, sitting next to the frequency select in the rule row.
- **Data model:** `channels TEXT[]` (subset of `{email, telegram}`) on
  `notification_rule`. Future-proof; routing is set-membership. `both` ↔
  `[email, telegram]`.
- **Telegram handle UI:** stays, repackaged as a compact "Telegram-Konto" card
  above the rules, no enable toggle. The email card disappears entirely (email
  comes from the user's login).
- **"Active user" redefined:** a user is active iff they have ≥1 rule with
  `frequency <> 'never'`. Replaces the old
  `email_enabled OR (telegram_enabled AND telegram_chat_id IS NOT NULL)` filter.
- **Unverified Telegram:** allow saving rules that reference telegram; the
  backend silently skips the telegram part of routing until the handle is
  verified (current `chat_id IS NULL` behavior). The UI shows a small warning
  badge on the rule and on the Telegram card.

## Design

### 1. Data model — migration `0007_channel_per_rule.sql`

```sql
-- 1. Per-rule channel array
ALTER TABLE notification_rule
  ADD COLUMN channels TEXT[] NOT NULL DEFAULT '{email}';

-- 2. Backfill each rule's channels from its user's old enablement
UPDATE notification_rule r
SET channels = CASE
  WHEN p.email_enabled AND p.telegram_enabled THEN ARRAY['email','telegram']::text[]
  WHEN p.email_enabled                         THEN ARRAY['email']::text[]
  WHEN p.telegram_enabled                      THEN ARRAY['telegram']::text[]
  ELSE                                              ARRAY['email']::text[]
END
FROM notification_preferences p
WHERE p.user_id = r.user_id;

-- 3. Drop the now-redundant global enablement columns
ALTER TABLE notification_preferences
  DROP COLUMN email_enabled,
  DROP COLUMN telegram_enabled;
```

Open pending batches keep their own `layer` column, so dropping the prefs
columns does not affect in-flight batches.

### 2. Backend

**`notification/rules.rs`**
- `Rule` gains `pub channels: Vec<String>`.
- `first_match` returns `Option<&Rule>` instead of `Option<&str>` so the caller
  can read both `frequency` and `channels`. Update its tests accordingly.

**`notification/db.rs`**
- `NotificationRule` and `RuleInput` gain `channels: Vec<String>`.
- `UserRules` drops `email_enabled` and `telegram_enabled` (keeps
  `telegram_chat_id`, `digest_anchor`, `digest_hour`, `rules`).
- `list_rules`, `replace_rules` SELECT/INSERT include `channels`.
- `list_active_users_with_rules` new query:

  ```sql
  SELECT p.user_id, p.telegram_chat_id, p.digest_anchor, p.digest_hour
  FROM notification_preferences p
  WHERE EXISTS (
    SELECT 1 FROM notification_rule r
    WHERE r.user_id = p.user_id AND r.frequency <> 'never'
  )
  ORDER BY p.user_id
  ```

  Then load rules per user as today.

**`notification/batch.rs::route_showing_for_users`**
- `first_match` now returns the matched `&Rule`. If `frequency == "never"` skip.
- If `"email" ∈ rule.channels` → open/create email batch with `rule.frequency`,
  append the showing.
- If `"telegram" ∈ rule.channels && u.telegram_chat_id.is_some()` → open/create
  telegram batch, append the showing. (Silently skip telegram when unverified;
  UI warns.)

**`notification/api.rs`**
- `RuleRequest` and `RuleResponse` gain `channels: Vec<String>`.
- `validate_rules`: `channels` must be a non-empty subset of `{email, telegram}`;
  reject otherwise (HTTP 400). Existing feature/title/frequency checks unchanged.
- `PreferenceUpdateRequest` and `PreferencesResponse` drop `email_enabled` and
  `telegram_enabled`. Only `telegramHandle`, `digestAnchor`, `digestHour` remain.
- `PreferenceUpdate` (db struct) drops the two enablement fields.
- `upsert_preferences` drops the two enablement binds and the `RETURNING` /
  `INSERT` columns.
- `put_preferences` no longer special-cases enablement for rollover; the
  telegram-handle-change rollover (existing) is kept. Saving rules still rolls
  over both layers' open batches (existing).
- `delete_telegram` still sets telegram disabled-equivalent state by clearing
  the handle (already does `telegram_handle: Some(String::new())`).

**`web.rs`** — no route changes.

### 3. Frontend

**`types.ts`**
- `NotificationPreferences` drops `emailEnabled`, `telegramEnabled`.
- `NotificationRule` gains `channel: "email" | "telegram" | "both"` (frontend
  single value; the API layer converts to/from the wire `channels[]`).

**`api/preferences.ts`**
- `savePreferences` body sends only `{ telegramHandle }` (and digest fields if
  ever exposed).
- `saveRules`: map `channel` → `channels` array:
  `email → ["email"]`, `telegram → ["telegram"]`, `both → ["email","telegram"]`.
- `fetchRules` (via `RulesResponse` decode): map `channels` array back to
  `channel` — `["email","telegram"] → "both"`, `["telegram"] → "telegram"`,
  else `"email"`.
- `NotificationRule` wire shape: `{ cinemaId, features, titleSubstring, frequency, channels }`.

**`pages/PreferencesPage.tsx`**
- Delete the email/telegram toggle cards (current lines 67-112).
- Replace with a compact **Telegram-Konto** card: handle input +
  `telegramVerified` / `telegramVerifyPrompt` status, no enable checkbox.
- Add a **channel `<select>`** (Email / Telegram / Both) to each rule row,
  next to the frequency select.
- `addRule` default: `channel: "both"` (most useful; UI warns if telegram
  unverified).
- Wrap the Add-rule and Save-rules buttons in a `.pref-actions` div (matching
  the top save bar) so they have proper spacing.
- Show a `.rule-warn` badge on rules selecting Telegram/Both when
  `!prefs.telegramVerified`, e.g. "Telegram nicht verknüpft".

**`index.css`** — add the missing rule-card styles, matching the existing
gold-on-dark cinema theme:
- `.rule-row`: `display:flex; flex-wrap:wrap; gap:.5rem; align-items:center`.
- `.rule-row select`, `.rule-row input`: reuse the `pref-input`/`pref-select`
  look (`background:var(--bg); border:1px solid var(--edge); color:var(--text);
  border-radius:4px; padding:.3rem .5rem; font-size:.8rem`).
- `.rule-features`: `display:flex; flex-wrap:wrap; gap:.35rem; margin-top:.5rem`.
- `.chip`: unstyled-toggle base — `background:var(--panel); border:1px solid
  var(--edge); color:var(--dim); border-radius:999px; padding:.2rem .6rem;
  font-size:.72rem; cursor:pointer; transition:...`.
- `.chip-on`: `background:rgba(232,179,77,.18); border-color:var(--gold);
  color:var(--gold-bright)`.
- `.rule-remove` (rename from `mock-button`): small danger button — `background:
  transparent; border:1px solid var(--edge); color:var(--err); border-radius:4px;
  padding:.15rem .45rem; font-size:.75rem; cursor:pointer`.
- `.rule-warn`: small badge — `color:var(--err); font-size:.7rem;
  margin-left:.4rem`.
- Responsive: at `max-width:560px`, `.rule-row` stacks selects/inputs full-width
  and the remove button sits at the row end.

**`pages/PreferencesPage.test.tsx`**
- `mockPrefs`: drop `emailEnabled`/`telegramEnabled`.
- Replace the "renders both channels with enable toggles" test with an assertion
  that the Telegram handle input is present (and reflects `telegramHandle`).
- Add a test that adding a rule, choosing a channel, and saving sends the
  expected `channels` array on the wire.
- Keep the load-error test as-is.

**locales `en.json` / `de.json`**
- Add under `preferences`:
  - `channel` ("Channel" / "Kanal")
  - `channelEmail` ("Email" / "E-Mail")
  - `channelTelegram` ("Telegram" / "Telegram")
  - `channelBoth` ("Both" / "Beide")
  - `telegramUnverified` ("Telegram not linked — this rule will only email until you verify." / "Telegram nicht verknüpft — diese Regel benachrichtigt nur per E-Mail, bis du verknüpfst.")
- Soften `telegramDesc`: drop "to activate" toggle language; describe linking
  the account so telegram rules can fire.

### 4. Tests (backend)

- `notification/api.rs`:
  - Update `put_preferences_*` request bodies to drop `emailEnabled`/
    `telegramEnabled`.
  - Update `get_preferences_defaults` to not assert `emailEnabled`/
    `telegramEnabled`.
  - Update `put_rules_replaces_and_rolls_over` body to include `channels`.
  - Add `put_rules_rejects_empty_channels` and `put_rules_rejects_invalid_channel`.
- `notification/batch.rs`:
  - Update `rule()`/`user_rules()` helpers to include `channels`.
  - Update existing routing tests to set `channels` explicitly.
  - Add `telegram_only_rule_with_chat_id_routes_telegram_batch`.
  - Add `telegram_only_rule_without_chat_id_routes_nothing`.
  - Add `both_channels_routes_email_and_telegram_batches`.
- `notification/db.rs`:
  - Update `prefs_for` helper (drop enablement args).
  - Update `list_active_users_with_rules_filters_inactive` expectations
    (active = has a non-never rule).
  - Update `replace_rules_*` round-trips to include and assert `channels`.

### 5. Verification

- `cd backend && cargo test` (needs `docker compose up -d db`).
- `cd frontend && npm test && npm run build`.
- `cd backend && cargo fmt --check && cargo clippy -- -D warnings`.

## Migration / rollout notes

- The backfill is deterministic from existing enablement state; no data loss.
- Pending `notification_batch` rows are untouched (they carry their own
  `layer`), so users mid-digest are not interrupted.
- The `list_active_users_with_rules` semantics change means a user whose only
  non-never rule has `channels=['telegram']` but who is unverified will be
  loaded but route to nothing — same effective behavior as today's
  `telegram_enabled && chat_id IS NULL` filter (which was *excluded* by the
  old query). Net effect: a few extra no-op user loads; acceptable. Could be
  tightened later by adding `AND NOT EXISTS (... telegram-only unverified ...)`
  if it matters.
