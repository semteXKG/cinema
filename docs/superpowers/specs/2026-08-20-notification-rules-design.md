# Notification Rules — Design Spec

**Date:** 2026-08-20
**Status:** Draft

## Overview

Replace each user's single per-channel notification frequency with an ordered,
per-user **rule list**. Rules are evaluated top-to-bottom; the first matching
rule wins and its frequency governs how that showing is batched for that user.
A catch-all ("any cinema, any room, every 3 days") sits last; more specific
rules above it override the cadence ("Megaplex + any premium format → sofort").

The existing digest/batch/send engine is reused. The only engine change is
keying open batches by frequency in addition to `(user, layer)`.

The public `@ov_linz` broadcast is **untouched** — it still pushes every new
showing immediately. Rules govern only per-user batches.

## Decisions (from brainstorming)

1. **Per-user rules** (not the public channel, not a shared engine).
2. **Normalized features** per showing (`IMAX`, `Atmos`, `OV`, …) extracted at
   fetch time — unifies the asymmetric cinema data (Megaplex encodes format in
   `version`; Cineplexx in `technologies`/`conceptAttributesNames`, currently
   discarded by the fetcher).
3. **Match dimensions:** cinema (any/specific) + features (any-of) + optional
   movie-title substring. Genres deferred.
4. **Feature semantics: OR (any-of).** A rule with features `{IMAX, Atmos}`
   matches if the showing has *any* of them. Empty features = no constraint.
   Chosen over AND because the dominant use case is "any premium format →
   sofort"; AND-only would split that into multiple rules.
5. **Batching: per-frequency batches** (Approach A). `notification_batch` gains
   a `frequency` column; the unique-open-batch index becomes
   `(user_id, layer, frequency)`.
6. **Layer model:** per-layer *enablement* (email on/off, telegram on/off)
   replaces per-layer *frequency*. A rule's frequency applies to both enabled
   layers. (Trade-off: loses independent email-vs-telegram frequency; acceptable
   for v1.)
7. **UI:** compact rule rows (summary line, expand to edit, drag-to-reorder).

## Requirements

1. Extract and persist a normalized `features` set on each `showing`.
2. Persist an ordered rule list per user: cinema (any/specific), features
   (any-of), optional title substring, frequency (`never` | `immediately` |
   `1`–`7`).
3. On each checker run, evaluate each active user's rules against each new
   showing; route the showing into the `(user, layer, matched_frequency)` open
   batch for each enabled layer. `never` matches route nowhere (silent
   suppression); no-match defaults to `never`.
4. Flush batches per the existing due logic, now reading frequency off the
   batch row. `immediately` flushes in the same run; `N` flushes at the next
   digest.
5. Authenticated API to read/replace the rule list, with validation and batch
   rollover on save.
6. Frontend rule editor on `PreferencesPage`.
7. Migrate existing per-layer frequencies to a seeded catch-all rule; preserve
   immediacy.

## Data Model

### New column: `showing.features`

```sql
ALTER TABLE showing ADD COLUMN features TEXT[] NOT NULL DEFAULT '{}';
```

Computed at fetch time (mirrors `movie.genres`). `insert_showing` gains a
`features: &[&str]` parameter. Existing rows backfill to `'{}'` (transient; only
new showings carry real features).

Feature vocabulary (case-insensitive extraction):

`OV`, `OmU`, `OmdU`, `2D`, `3D`, `IMAX`, `Atmos`, `DolbyCinema`, `4DX`

Extraction scans the combined text of `version` + `hall` (Megaplex) and the
`technologies` + `conceptAttributesNames` JSON arrays (Cineplexx — the fetcher
enhancement to keep detail it currently drops at `cineplexx.rs:92`).

Examples:

- Megaplex `"OV - IMAX 2D"` → `{OV, IMAX, 2D}`
- Megaplex `"OV - Dolby Atmos"` → `{OV, Atmos}` (Atmos = sound; DolbyCinema =
  vision, a distinct feature)
- Cineplexx `technologies ["2D","OV (Englisch)"]`, `hall "Saal 6"` → `{OV, 2D}`

> Extraction token rules (pinned by unit tests): `(?i)\bOV\b`→OV,
> `\bOmU\b`→OmU, `\bOmdU\b`→OmdU, `\bIMAX\b`→IMAX,
> `\bDolby\s+Atmos\b|\bAtmos\b`→Atmos, `\bDolby\s+Cinema\b|\bDolby\s+Vision\b`→DolbyCinema,
> `\b3D\b`→3D, `\b2D\b`→2D, `\b4DX\b`→4DX. Applied to the lowercased combined
> string with word boundaries. `"Dolby Atmos"` → `{Atmos}` (sound);
> `"Dolby Cinema"`/`"Dolby Vision"` → `{DolbyCinema}` (vision). They are
> independent features — a screening can carry both.

### New table: `notification_rule`

```sql
CREATE TABLE notification_rule (
  id              BIGSERIAL PRIMARY KEY,
  user_id         BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  position        INT NOT NULL,
  cinema_id       BIGINT REFERENCES cinema(id),   -- NULL = any cinema
  features        TEXT[] NOT NULL DEFAULT '{}',    -- any-of; empty = any
  title_substring TEXT,                           -- NULL/empty = any title
  frequency       TEXT NOT NULL,                  -- never|immediately|1..7
  created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (user_id, position)
);
CREATE INDEX idx_notification_rule_user ON notification_rule(user_id, position);
```

### Updated: `notification_batch`

```sql
ALTER TABLE notification_batch ADD COLUMN frequency TEXT NOT NULL DEFAULT 'immediately';

-- backfill pending batches from each owner's current prefs (see Migration)
-- then:
DROP INDEX idx_batch_open_unique;
CREATE UNIQUE INDEX idx_batch_open_unique
  ON notification_batch(user_id, layer, frequency) WHERE status = 'pending';
```

`frequency` is now a property of the batch; all showings in a batch share the
matched frequency. `get_or_create_open_batch` and `create_empty_batch` take a
`frequency` argument.

### Updated: `notification_preferences`

```sql
ALTER TABLE notification_preferences
  ADD COLUMN email_enabled    BOOL NOT NULL DEFAULT false,
  ADD COLUMN telegram_enabled BOOL NOT NULL DEFAULT false;
-- backfill from email_frequency / telegram_frequency (see Migration)
ALTER TABLE notification_preferences
  DROP COLUMN email_frequency,
  DROP COLUMN telegram_frequency;
```

Keeps: `telegram_handle`, `telegram_chat_id`, `digest_anchor`, `digest_hour`,
`updated_at`. `digest_anchor`/`digest_hour` are shared across all of a user's
N-day batches (one digest window per user).

## Matching Semantics

A rule `R` matches a showing `S` (with `cinema_id C_s`, `features F_s`,
`title T`) iff **all** of:

- `R.cinema_id IS NULL OR R.cinema_id = C_s`
- `R.features = '{}' OR array_overlap(R.features, F_s)` (any-of)
- `R.title_substring IS NULL OR btrim(R.title_substring) = '' OR S.title ILIKE '%' || R.title_substring || '%'`

Rules are evaluated in `position ASC` order. The first match wins; its
`frequency` applies. **No match → `never`** (suppress, route nowhere).

Matching is performed in Rust (rule lists are tiny; users few). The DB returns
each active user's rule list pre-sorted by `position`, plus the matchable
attributes per showing.

## Routing & Batching Data Flow

Replaces `append_showing_for_users` in `checker.rs:251-257`.

1. `list_active_users_with_rules(pool)` — one query returning each enabled user
   (`email_enabled`, `telegram_enabled`, `telegram_chat_id`, shared
   `digest_anchor`/`digest_hour`) with their rule list sorted by `position`.
   Replaces `list_active_preferences`. Active = `email_enabled` OR
   `(telegram_enabled AND telegram_chat_id IS NOT NULL)`.
2. `load_matchable_showings(pool, &[showing_id])` — single batched
   `showing → movie → cinema` join returning `{showing_id, cinema_id, features,
   title}` per new showing (uses the stored `features` column).
3. For each `(showing, user)`: scan that user's rules in `position` order;
   first match → frequency; no match → `never`. For each enabled layer, if
   frequency ≠ `never`, route into the `(user, layer, frequency)` open batch.
   `never` → skip (no batch created).

`get_due_batches` (`notification/db.rs:170`): frequency now comes from
`b.frequency` instead of the `COALESCE(CASE…)` expression over preferences.
`digest_anchor`/`digest_hour` still join `notification_preferences`.

`batch_is_due` / `next_digest_after` (`schedule.rs`): **unchanged** —
immediately → always due; `N` → next digest strictly after `created_at`.

`handle_batch` (`batch.rs:109`): **unchanged** flush lifecycle — load showings,
filter `movie_ignore`, format, send, `mark_batch_sent`, then
`create_empty_batch(user, layer, frequency)` for the next cycle. The unique
index keeps one open batch per `(user, layer, frequency)`.

Error handling unchanged: failed batches retry with exponential backoff
(`get_due_batches` retry clause); `gc_failed_batches` reaps old failures; a
showing in a failed batch retries when the batch becomes retryable.

## API

### Base preferences (`/api/preferences`, unchanged routes)

`PUT`/`GET` drop `emailFrequency`/`telegramFrequency`; replace with
`emailEnabled`/`telegramEnabled` (bool). `telegramHandle`, `telegramVerified`,
`digestAnchor`, `digestHour` stay. `PUT` still rolls over open batches when
digest settings change.

### Rules — replace-whole-list style (mirrors `savePreferences`)

- `GET /api/preferences/rules` →
  `{ rules: [{id, position, cinemaId?, cinemaName?, features[], titleSubstring?, frequency}], cinemas: [{id, name}] }`
  (bundles known cinemas for the picker).
- `PUT /api/preferences/rules` with the full ordered array → atomically replaces
  the user's rules (delete+insert in one transaction; `position` = array index).
  On success, rolls over (deletes) the user's open batches so nothing sends
  against stale routing — same semantics as today's frequency-change rollover.

Validation (400 on violation):

- `frequency` ∈ {`never`, `immediately`, `1`–`7`}
- `cinemaId` null or exists in `cinema`
- `features` subset of the fixed vocabulary
- `titleSubstring` ≤ 200 chars
- ≤ 32 rules per user

Auth: same `AuthUser` cookie session (`api.rs`); 401 without session.

## Frontend

Rule editor on `PreferencesPage`, compact-row layout (option B from the
mockups):

- Each rule = one row showing a summary line:
  `<cinema> · <features> · <title?> → <frequency>` with a drag handle (≡),
  Edit button, and delete (✕).
- Edit expands the row inline: cinema `<select>` (Any / Cineplexx Linz /
  Megaplex PlusCity), feature toggle chips (the vocabulary), title `<input>`,
  frequency `<select>` (Immediately / Every 1–7 days / Never).
- Reorder via drag (HTML drag-and-drop or up/down buttons as fallback).
- `+ Regel hinzufügen` appends a new catch-all-style row at the end.
- Save sends the full ordered list via `PUT /api/preferences/rules`.

The existing per-layer cards remain (email/telegram enable + telegram handle
verification + digest settings), with frequency selects replaced by enable
toggles.

Feature chip rendering: selected chips highlighted; the rule's any-of semantics
communicated by a small label ("mindestens eins") on the features row.

## Migration (`0006_notification_rules.sql`)

1. `ALTER TABLE showing ADD COLUMN features TEXT[] NOT NULL DEFAULT '{}';`
   (existing rows stay `'{}'`).
2. `ALTER TABLE notification_batch ADD COLUMN frequency TEXT NOT NULL DEFAULT 'immediately';`
3. Backfill each pending batch's frequency from its **own layer's** old pref
   (preserves each batch's intended cadence — a queued 3-day email batch stays
   3-day even if the user's telegram was immediate):
   ```sql
   UPDATE notification_batch b SET frequency = COALESCE(
     CASE WHEN b.layer = 'email' THEN p.email_frequency ELSE p.telegram_frequency END,
     'never')
   FROM notification_preferences p
   WHERE p.user_id = b.user_id AND b.status = 'pending';
   ```
   (A pending batch only exists for a layer that was non-never, so this always
   yields a non-never frequency.)
4. `DROP INDEX idx_batch_open_unique;` then recreate on
   `(user_id, layer, frequency) WHERE status='pending'`.
5. `CREATE TABLE notification_rule (…)`.
6. `ALTER TABLE notification_preferences ADD COLUMN email_enabled BOOL NOT NULL DEFAULT false,
   ADD COLUMN telegram_enabled BOOL NOT NULL DEFAULT false;`
7. Backfill enablement:
   `UPDATE notification_preferences SET email_enabled = (email_frequency <> 'never'),
   telegram_enabled = (telegram_frequency <> 'never');`
8. Seed one catch-all rule per user with any non-never frequency, using
   cross-layer urgency (immediately preferred across both layers):
   ```sql
   INSERT INTO notification_rule (user_id, position, cinema_id, features, title_substring, frequency)
   SELECT user_id, 0, NULL, '{}', NULL,
     CASE WHEN email_frequency='immediately' OR telegram_frequency='immediately'
          THEN 'immediately'
          WHEN email_frequency <> 'never' THEN email_frequency
          WHEN telegram_frequency <> 'never' THEN telegram_frequency
          ELSE '3' END
   FROM notification_preferences
   WHERE email_frequency <> 'never' OR telegram_frequency <> 'never';
   ```
   Users with both layers `never` get no rule (they were inactive).
9. `ALTER TABLE notification_preferences DROP COLUMN email_frequency,
   DROP COLUMN telegram_frequency;`

After migration, existing pending batches flush per their backfilled frequency
(consistent with the seeded catch-all rule); new showings route by rules.

## Defaults

- New user: `email_enabled=false`, `telegram_enabled=false`, no rules → nothing
  routed (no match = `never`). Must enable a layer and add rules.
- Feature extraction computed in the fetchers; `insert_showing` receives it.

## Testing

- **Feature extraction** — pure-fn unit tests with golden values (mirroring
  `cineplexx_session_version` tests in `models.rs`): `"OV - IMAX 2D"` →
  `{OV, IMAX, 2D}`; Cineplexx `technologies`/`conceptAttributesNames` fixtures
  → correct set; `Dolby Atmos` → `{Atmos}` (not `DolbyCinema`).
- **Rule matching** — unit tests on `matches(rule, attrs) -> bool` and
  `first_match(rules, attrs) -> Option<Frequency>`: empty features = match-all,
  any-of overlap, cinema NULL/specific, title ILIKE, no-match → `None`.
- **Routing (`sqlx::test`)** — seed user + rules + enabled layers, insert a
  showing with features, assert it lands in the right `(user, layer, frequency)`
  batch; `never`-match creates no batch; disabled layer creates no batch.
- **Due/flush (`sqlx::test`)** — extend `batch.rs` tests: immediately batch
  flushes same run, 3-day batch flushes at digest, multiple per-frequency
  batches coexist.
- **API (axum oneshot)** — GET/PUT rules auth + validation (bad frequency,
  unknown cinema, bad feature, >32 rules), replace semantics, rollover deletes
  open batches.
- **Migration** — `sqlx::test` runs cleanly; backfilled
  `email_enabled`/`telegram_enabled` correct; catch-all rule seeded for
  existing prefs with preserved immediacy.
- **Frontend (Vitest)** — rule editor add/reorder/edit/delete, compact-row
  expand, feature chips, save; mirroring `PreferencesPage.test.tsx`.

## Out of Scope

- Rules for the public `@ov_linz` broadcast (it stays immediate + unfiltered).
- Genre matching.
- Per-layer rule sets / per-layer frequencies (v1 applies a rule's frequency to
  both enabled layers).
- AND feature semantics (per-rule ANY/ALL toggle) — deferred; OR covers the
  dominant premium-format-catch case.
- Rule import/export or sharing.
