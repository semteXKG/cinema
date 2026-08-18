# Notification Batching — Design Spec

**Date:** 2026-08-18  
**Status:** Draft

## Overview

Add per-user notification support for the OV-Kino Linz watcher. The existing
public Telegram channel (`@ov_linz`) keeps working unchanged. We add two new
per-user notification layers:

- **Email** — sent to the address stored in `users.email`.
- **Telegram DM** — sent to a verified Telegram chat id.

Users choose a frequency per layer:

- `never` — no notifications.
- `immediately` — send a batch as soon as a new showing is discovered.
- `1` … `7` — send a digest every N days at a configurable wall-clock hour
  (default 09:00 Europe/Vienna).

This spec covers both the missing persistence/API for preferences and the
batching/sending engine.

## Requirements

1. Persist notification preferences per user (email frequency, Telegram
   frequency, Telegram handle, verified chat id, digest anchor, digest hour).
2. Provide authenticated API endpoints for the preferences page.
3. Verify Telegram handles via a bot webhook so we can obtain the user's
   `chat_id`.
4. During every checker run, create/append notification batches for newly
   discovered showings.
5. Send due batches according to each user's frequency/schedule.
6. Reuse the existing SMTP transport for email and the existing bot token for
   Telegram DMs.
7. Keep the public `@ov_linz` channel sender untouched.

## Architecture

```
frontend PreferencesPage  <--->  backend notification module
                                        |
        +-----------------------------+-----------------------------+
        |                             |                             |
   preferences API          Telegram webhook            batching engine
   (GET/PUT /api/           (POST /api/telegram/       (invoked by
    preferences)             webhook/<secret>)          checker)
        |                             |                             |
        v                             v                             v
notification_preferences    notification_preferences    notification_batch
                                                   +     notification_batch_showing
                                                   |
                                                   v
                                            email / telegram DM
```

Three new backend concerns live under a `notification` module:

1. **Preferences persistence** — DB helpers and API handlers.
2. **Telegram verification** — webhook handler that records `telegram_chat_id`.
3. **Batching engine** — creates batches during the checker run and sends them.

## Data model

### `notification_preferences`

One row per user.

```sql
CREATE TABLE notification_preferences (
  user_id              BIGINT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
  email_frequency      TEXT NOT NULL DEFAULT 'never',
  telegram_frequency   TEXT NOT NULL DEFAULT 'never',
  telegram_handle      TEXT,            -- normalized: no leading @, lowercased
  telegram_chat_id     TEXT,            -- NULL until verified
  digest_anchor        TIMESTAMPTZ NOT NULL DEFAULT now(),
  digest_hour          INT NOT NULL DEFAULT 9 CHECK (digest_hour BETWEEN 0 AND 23),
  updated_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

Frequency values: `never`, `immediately`, `1`, `2`, `3`, `4`, `5`, `6`, `7`.

### `notification_batch`

One open (`pending`) batch per user/layer. Closed rows are kept as history.

```sql
CREATE TABLE notification_batch (
  id           BIGSERIAL PRIMARY KEY,
  user_id      BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  layer        TEXT NOT NULL CHECK (layer IN ('email', 'telegram')),
  status       TEXT NOT NULL DEFAULT 'pending'
               CHECK (status IN ('pending', 'sending', 'sent', 'failed')),
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  sent_at      TIMESTAMPTZ,
  error_count  INT NOT NULL DEFAULT 0,
  last_error   TEXT
);

CREATE UNIQUE INDEX idx_batch_open_unique
  ON notification_batch(user_id, layer)
  WHERE status = 'pending';

CREATE INDEX idx_batch_status ON notification_batch(user_id, layer, status)
  WHERE status IN ('pending', 'sending', 'failed');
```

### `notification_batch_showing`

Links showings to batches.

```sql
CREATE TABLE notification_batch_showing (
  batch_id     BIGINT NOT NULL REFERENCES notification_batch(id) ON DELETE CASCADE,
  showing_id   BIGINT NOT NULL REFERENCES showing(id) ON DELETE CASCADE,
  added_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (batch_id, showing_id)
);
```

## Preferences API

All endpoints require an authenticated session.

### `GET /api/preferences`

Response (`200 OK`):

```json
{
  "emailFrequency": "immediately",
  "telegramFrequency": "3",
  "telegramHandle": "myhandle",
  "telegramVerified": true,
  "digestAnchor": "2026-08-09T09:00:00+02:00",
  "digestHour": 9
}
```

`telegramVerified` is derived from `telegram_chat_id IS NOT NULL`.
If the user has no row yet, return defaults:
`emailFrequency: "never"`, `telegramFrequency: "never"`, empty handle,
`telegramVerified: false`, `digestAnchor` defaults to `users.created_at`,
`digestHour: 9`.

### `PUT /api/preferences`

Request body accepts any subset of fields:

```json
{
  "emailFrequency": "immediately",
  "telegramFrequency": "3",
  "telegramHandle": "@MyHandle",
  "digestAnchor": "2026-08-09T07:00:00Z",
  "digestHour": 9
}
```

Validation rules:

- `emailFrequency` and `telegramFrequency` must be one of the allowed values.
- `telegramHandle` is normalized: strip leading `@`, trim whitespace, lowercase.
  Setting it to `null` or empty string clears both `telegram_handle` and
  `telegram_chat_id`.
- `digestAnchor` is stored as UTC.
- `digestHour` must be 0–23.

Side effects:

- If `emailFrequency` or `digestAnchor`/`digestHour` changes, delete the current
  open email batch (and its linked showing rows) and create a fresh empty one.
- Same for Telegram.
- If `telegramHandle` is cleared, also clear `telegram_chat_id`, set
  `telegram_frequency = 'never'`, and delete open Telegram batches.

Response (`200 OK`): the updated preferences object.

### `DELETE /api/preferences/telegram`

Convenience endpoint to disable Telegram: clears handle, chat id, and frequency
(`telegram_frequency = 'never'`). Deletes any open Telegram batch and its links.

## Telegram verification flow

Telegram bots cannot send DMs by username; they need a numeric `chat_id`. The
verification flow obtains it.

1. User enters a handle on `/preferences` and clicks Save.
2. Backend stores the normalized handle with `telegram_chat_id = NULL`.
3. User sends `/start` (or any message) to the bot.
4. Telegram POSTs the update to `POST /api/telegram/webhook/<secret>`.
5. Webhook handler:
   - Validates the path secret.
   - Extracts `message.from.username` and `message.chat.id`.
   - Normalizes the username.
   - Looks up `notification_preferences` where `telegram_handle` matches and
     `telegram_chat_id IS NULL`.
   - If found, sets `telegram_chat_id` and replies with a confirmation message
     in German/English.
   - If not found, replies with instructions to enter the handle on the site
     first.
6. Frontend polls `GET /api/preferences` to detect `telegramVerified: true`.

### Webhook security

The webhook URL contains a secret token configured via env var
`TELEGRAM_WEBHOOK_SECRET`. The bot's webhook is set to
`https://<base_url>/api/telegram/webhook/<TELEGRAM_WEBHOOK_SECRET>`.
Requests with a mismatched path secret are rejected with `401 Unauthorized`.

## Batching engine

The batching engine runs inside `checker::run_check` after new showings have
been persisted.

### 1. Append new showings to open batches

For each newly inserted `showing`:

- Find or create the user's open email batch if `email_frequency != 'never'`.
- Find or create the user's open Telegram batch if
  `telegram_frequency != 'never'` **and** `telegram_chat_id IS NOT NULL`.
- Insert into `notification_batch_showing` with `ON CONFLICT DO NOTHING` for
  idempotency.

A helper returns the list of affected `(user_id, layer)` pairs so the next step
knows which batches to evaluate.

### 2. Send due batches

A batch is due when:

- `frequency = 'immediately'`, OR
- `frequency = 'N'` and the computed next digest time is `<= now()`.

The next digest time is computed from the user's `digest_anchor` and
`digest_hour`, stepping forward by `frequency_days` until it exceeds the batch
`created_at`. If that time is `<= now()`, the batch is due.

For each due batch:

1. Transition status `pending` → `sending` and update `updated_at`.
2. Load all linked showings joined with `movie` metadata.
3. Format the message using `notify::format_message`.
4. Send via the appropriate notifier.
5. On success: mark `sent`, set `sent_at`, insert a new empty `pending` batch
   for that user/layer.
6. On failure: mark `failed`, increment `error_count`, store `last_error`,
   update `updated_at`, leave the batch open for retry.

### 3. Message formatting

- Telegram DM: use `notify::format_message` directly (Telegram HTML subset).
- Email: wrap the same HTML in a minimal email body with a small footer
  containing a link to `/preferences`.
- Subject: `"Neue OV-Vorstellungen in Linz"`.

## Notifiers

### Email

Reuse the existing SMTP transport builder from `auth.rs`. A new
`EmailNotifier` implements the existing `Notifier` trait or a similar one,
sending HTML email to the user's `users.email` address. The from address is
`showings@<domain>` where `<domain>` is derived from `base_url` and can be
overridden via env var `NOTIFICATION_EMAIL_FROM`.

### Telegram DM

Generalize the existing `TelegramNotifier` so the destination `chat_id` is
provided per send, or create a `TelegramDmNotifier` that sends to a supplied
`chat_id`. The public channel sender keeps its own hard-coded `chat_id`
(`TELEGRAM_CHAT_ID`) and remains unchanged.

## Error handling & retries

- Sending failures do not crash the checker. Errors are logged and the batch is
  marked `failed`.
- Retry schedule: after `2^error_count` hours, capped at 24 hours.
- During each checker run, query also includes `failed` batches whose
  `updated_at + retry_delay <= now()`.
- If a batch has been failing for more than 7 days, drop it and open a fresh
  empty batch to avoid infinite retries.
- Empty batches (all linked showings pruned, or no showings ever appended) are
  skipped, not sent.

## Edge cases

- User changes frequency or digest settings: delete the open batch and its
  links, then open a new empty one. Showings already sent are unaffected.
- User clears Telegram handle: clear `telegram_chat_id`, set frequency to
  `never`, delete open Telegram batches and their links.
- User account deletion: `ON DELETE CASCADE` removes preferences and batches.
- Showing pruned before send: `ON DELETE CASCADE` on
  `notification_batch_showing` removes the link. Empty batches are skipped.
- No users with notifications enabled: batching step is a no-op.

## Testing

### Backend

- Unit tests for the digest scheduling helper (`next_digest_after`).
- `#[sqlx::test]` for:
  - `GET /api/preferences` returns defaults and persisted values.
  - `PUT /api/preferences` normalizes the Telegram handle and triggers batch
    rollover on schedule changes.
  - Appending a new showing creates open batches only for enabled layers.
  - `immediately` frequency sends the batch during the same checker run.
  - `N days` frequency sends only when the schedule is due.
  - Telegram DM batch is not created for unverified handles.
  - Failed batches retry with backoff and are dropped after the max age.
- Webhook handler test with a mocked Telegram update JSON.

### Frontend

- Preferences page loads and saves via the API.
- Telegram handle input is normalized on save (frontend may strip `@` before
  sending or rely on the backend).
- Verified state is shown when `telegramVerified` is true.
- Save button shows feedback.

## Out of scope

- Migrating or removing the public `@ov_linz` channel.
- In-app Telegram deep-link flow beyond the webhook verification.
- Unsubscribe links with signed tokens (the preferences link is enough for
  MVP).
- Push notifications or other channels.

## Env vars

| Variable | Description |
|----------|-------------|
| `TELEGRAM_BOT_TOKEN` | Existing bot token. |
| `TELEGRAM_WEBHOOK_SECRET` | Secret path segment for the webhook endpoint. |
| `NOTIFICATION_EMAIL_FROM` | Override sender address (default `showings@<base_url_domain>`). |
| `NOTIFICATION_MAX_RETRY_AGE_HOURS` | Drop failed batches after this many hours (default 168). |
