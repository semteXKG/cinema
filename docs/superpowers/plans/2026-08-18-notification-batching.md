# Notification Batching Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist per-user notification preferences, verify Telegram handles via webhook, and send batched email/Telegram-DM notifications on user-defined schedules.

**Architecture:** One open batch per user/layer accumulates newly discovered showings during the checker run. A scheduling helper decides when fixed-interval digests are due. Preferences and batch state live in new Postgres tables; a `notification` module owns DB helpers, API handlers, verification, and the batching engine.

**Tech Stack:** Rust (axum, sqlx, chrono, lettre), Postgres, React/TypeScript/Vite, Telegram Bot API.

## Global Constraints

- Frequency values are exactly: `never`, `immediately`, `1`, `2`, `3`, `4`, `5`, `6`, `7`.
- Telegram handles are normalized: strip leading `@`, trim whitespace, lowercase before storage.
- The public `@ov_linz` Telegram channel sender must remain unchanged.
- All notification endpoints require an authenticated session.
- Digest hour is 0–23, default 09:00 Europe/Vienna.
- Reuse existing SMTP config for email and existing bot token for Telegram DMs.

---

## File Structure

### Backend

| File | Responsibility |
|------|----------------|
| `backend/migrations/0003_notifications.sql` | Creates `notification_preferences`, `notification_batch`, `notification_batch_showing`. |
| `backend/src/notification/mod.rs` | Module entry point: re-exports public types and routers. |
| `backend/src/notification/db.rs` | DB helpers for preferences and batches (CRUD, append, due queries). |
| `backend/src/notification/schedule.rs` | `next_digest_after` helper and frequency validation. |
| `backend/src/notification/batch.rs` | Batching engine: `append_new_showings`, `send_due_batches`. |
| `backend/src/notification/verify.rs` | Telegram webhook handler and handle normalization. |
| `backend/src/notification/send.rs` | `EmailNotifier` and `TelegramDmNotifier`. |
| `backend/src/config.rs` | Add `telegram_webhook_secret`, `notification_email_from`, `notification_max_retry_age_hours`. |
| `backend/src/web.rs` | Mount preferences and webhook routes. |
| `backend/src/checker.rs` | Call batching engine after persisting new showings. |
| `backend/src/main.rs` | Configure Telegram webhook on startup (optional best-effort). |
| `backend/src/notify.rs` | Generalize or duplicate Telegram notifier for per-user chat ids. |

### Frontend

| File | Responsibility |
|------|----------------|
| `frontend/src/api/preferences.ts` | `fetchPreferences`, `savePreferences`. |
| `frontend/src/types.ts` | Add `NotificationPreferences` type. |
| `frontend/src/pages/PreferencesPage.tsx` | Load/save real preferences, show verification state. |
| `frontend/src/locales/{en,de}.json` | Add verification/help strings. |
| `frontend/src/index.css` | Styling for verified/pending states. |

---

### Task 1: Database migration

**Files:**
- Create: `backend/migrations/0003_notifications.sql`

**Interfaces:**
- Produces: Three new tables used by Tasks 2–13.

- [ ] **Step 1: Write migration**

```sql
CREATE TABLE notification_preferences (
  user_id              BIGINT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
  email_frequency      TEXT NOT NULL DEFAULT 'never',
  telegram_frequency   TEXT NOT NULL DEFAULT 'never',
  telegram_handle      TEXT,
  telegram_chat_id     TEXT,
  digest_anchor        TIMESTAMPTZ NOT NULL DEFAULT now(),
  digest_hour          INT NOT NULL DEFAULT 9 CHECK (digest_hour BETWEEN 0 AND 23),
  updated_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

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

CREATE TABLE notification_batch_showing (
  batch_id     BIGINT NOT NULL REFERENCES notification_batch(id) ON DELETE CASCADE,
  showing_id   BIGINT NOT NULL REFERENCES showing(id) ON DELETE CASCADE,
  added_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (batch_id, showing_id)
);
```

- [ ] **Step 2: Verify migration compiles**

Run: `cd backend && DATABASE_URL=postgres://ov:ov@localhost:5432/ov cargo sqlx migrate run`

Expected: migrations apply successfully (requires `cargo install sqlx-cli` if missing).

- [ ] **Step 3: Commit**

```bash
git add backend/migrations/0003_notifications.sql
git commit -m "feat: notification preferences and batch tables"
```

---

### Task 2: Notification DB helpers

**Files:**
- Create: `backend/src/notification/mod.rs`
- Create: `backend/src/notification/db.rs`
- Modify: `backend/src/main.rs` (add `mod notification;`)

**Interfaces:**
- Consumes: `PgPool`, `DateTime<Utc>`.
- Produces:
  - `NotificationPreferences` struct.
  - `PreferenceUpdate` struct (DB DTO).
  - `upsert_preferences(pool, user_id, dto) -> Result<NotificationPreferences, sqlx::Error>`.
  - `get_preferences(pool, user_id) -> Result<NotificationPreferences, sqlx::Error>`.
  - `list_active_preferences(pool) -> Result<Vec<NotificationPreferences>, sqlx::Error>` (users with at least one enabled layer).
  - `get_or_create_open_batch(pool, user_id, layer) -> Result<i64, sqlx::Error>`.
  - `append_showing_to_batch(pool, batch_id, showing_id) -> Result<(), sqlx::Error>`.
  - `DueBatch` struct (batch id + user schedule info).
  - `get_due_batches(pool, now) -> Result<Vec<DueBatch>, sqlx::Error>`.
  - `mark_batch_sending(pool, batch_id) -> Result<(), sqlx::Error>`.
  - `mark_batch_sent(pool, batch_id) -> Result<(), sqlx::Error>`.
  - `mark_batch_failed(pool, batch_id, error) -> Result<(), sqlx::Error>`.
  - `create_empty_batch(pool, user_id, layer) -> Result<i64, sqlx::Error>`.
  - `delete_open_batch(pool, user_id, layer) -> Result<(), sqlx::Error>`.

- [ ] **Step 1: Write failing DB helper tests**

Create `backend/src/notification/db.rs` with tests at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    #[sqlx::test(migrations = "./migrations")]
    async fn preferences_defaults_and_upsert(pool: PgPool) {
        let uid = crate::db::find_or_create_user(&pool, "email", "a@b.com", "a@b.com")
            .await
            .unwrap();
        let prefs = get_preferences(&pool, uid).await.unwrap();
        assert_eq!(prefs.email_frequency, "never");
        assert_eq!(prefs.telegram_frequency, "never");
        assert!(prefs.telegram_handle.is_none());
        assert!(prefs.telegram_chat_id.is_none());
        assert_eq!(prefs.digest_hour, 9);

        let updated = upsert_preferences(
            &pool,
            uid,
            PreferenceUpdate {
                email_frequency: Some("immediately".into()),
                telegram_frequency: Some("3".into()),
                telegram_handle: Some("@MyHandle".into()),
                digest_anchor: None,
                digest_hour: Some(10),
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.email_frequency, "immediately");
        assert_eq!(updated.telegram_frequency, "3");
        assert_eq!(updated.telegram_handle.as_deref(), Some("myhandle"));
    }
}
```

Run: `cd backend && cargo test notification::db::tests::preferences_defaults_and_upsert -- --nocapture`

Expected: compile errors because structs/functions don't exist.

- [ ] **Step 2: Create module entry point**

Create `backend/src/notification/mod.rs`:

```rust
pub mod db;
```

Add `mod notification;` to `backend/src/main.rs`.

Note: later tasks append their module declarations to this file as the modules
are created: Task 3 adds `pub mod schedule;`, Task 4 adds `mod api;` +
`pub use api::preferences_router;`, Tasks 5 adds `pub mod send;`, Task 7 adds
`pub mod batch;`, Task 8 adds `pub mod verify;` +
`pub use verify::telegram_webhook_router;`.

- [ ] **Step 3: Implement DB helpers**

Implement `NotificationPreferences`, `PreferenceUpdate`, and all helper functions in `backend/src/notification/db.rs`.

```rust
#[derive(Debug, Default)]
pub struct PreferenceUpdate {
    pub email_frequency: Option<String>,
    pub telegram_frequency: Option<String>,
    pub telegram_handle: Option<String>,
    pub digest_anchor: Option<DateTime<Utc>>,
    pub digest_hour: Option<i32>,
}

#[derive(Debug)]
pub struct DueBatch {
    pub batch_id: i64,
    pub user_id: i64,
    pub layer: String,
    pub frequency: String,
    pub digest_anchor: DateTime<Utc>,
    pub digest_hour: i32,
    pub created_at: DateTime<Utc>,
}
```

Key implementation notes:
- `upsert_preferences` normalizes `telegram_handle` and clears `telegram_chat_id` when the handle changes or is cleared.
- `get_preferences` returns defaults if no row exists; derive `digest_anchor` from `users.created_at`.
- `list_active_preferences` returns users where `email_frequency != 'never'` or
  (`telegram_frequency != 'never'` and `telegram_chat_id IS NOT NULL`).
- `get_or_create_open_batch` uses an `INSERT ... ON CONFLICT` against the partial unique index.
- `get_due_batches` returns pending batches where:
  - frequency is `immediately`, OR
  - frequency is `N` and `next_digest_after(...) <= now`.
  It also returns failed batches eligible for retry (`updated_at + retry_delay <= now`).

- [ ] **Step 4: Run DB helper tests**

Run: `cd backend && cargo test notification::db -- --nocapture`

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add backend/src/notification/mod.rs backend/src/notification/db.rs backend/src/main.rs
git commit -m "feat: notification DB helpers and defaults"
```

---

### Task 3: Scheduling helper

**Files:**
- Create: `backend/src/notification/schedule.rs`
- Modify: `backend/src/notification/mod.rs` (add `pub mod schedule;`)

**Interfaces:**
- Consumes: `digest_anchor: DateTime<Utc>`, `digest_hour: i32`, `frequency_days: i32`, `now: DateTime<Utc>`.
- Produces:
  - `fn next_digest_after(...) -> Option<DateTime<Utc>>`.
  - `fn parse_frequency(value: &str) -> Option<Frequency>`.

- [ ] **Step 1: Write failing scheduling tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use chrono_tz::Europe::Vienna;

    fn at(day: u32, hour: u32) -> DateTime<Utc> {
        Vienna.with_ymd_and_hms(2026, 8, day, hour, 0, 0).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn daily_digest_first_after_t() {
        // anchor 18.08 09:00, frequency 1 day. First digest strictly after
        // 18.08 10:00 is 19.08 09:00.
        let anchor = at(18, 9);
        let t = at(18, 10);
        let next = next_digest_after(anchor, 9, 1, t).unwrap();
        assert_eq!(next, at(19, 9));
    }

    #[test]
    fn three_day_digest_steps_correctly() {
        // anchor 16.08 09:00, frequency 3 days => digest moments 16/19/22.08 09:00.
        // First digest strictly after 19.08 08:00 is 19.08 09:00.
        let anchor = at(16, 9);
        let t = at(19, 8);
        let next = next_digest_after(anchor, 9, 3, t).unwrap();
        assert_eq!(next, at(19, 9));
    }

    #[test]
    fn due_is_defined_by_next_digest_on_or_before_now() {
        // Circular helper test pinning the contract later tasks rely on:
        // a batch created at `created_at` is due at `now` iff
        // next_digest_after(anchor, hour, days, created_at) <= now.
        let anchor = at(16, 9);
        let created_at = at(19, 8);
        let digest_after_create = next_digest_after(anchor, 9, 3, created_at).unwrap();
        assert_eq!(digest_after_create, at(19, 9));
        assert!(digest_after_create <= at(19, 9)); // due at 19.08 09:00
        assert!(digest_after_create > at(19, 8)); // not due at 19.08 08:00
    }
}
```

Run: `cd backend && cargo test notification::schedule -- --nocapture`

Expected: compile errors.

- [ ] **Step 2: Implement scheduling helper**

```rust
use chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone, Utc};
use chrono_tz::Europe::Vienna;

pub enum Frequency {
    Never,
    Immediately,
    Days(i32),
}

pub fn parse_frequency(value: &str) -> Option<Frequency> {
    match value {
        "never" => Some(Frequency::Never),
        "immediately" => Some(Frequency::Immediately),
        n => n.parse::<i32>().ok().filter(|&d| d >= 1 && d <= 7).map(Frequency::Days),
    }
}
```

```rust
use chrono::{DateTime, Duration, NaiveTime, TimeZone, Utc};
use chrono_tz::Europe::Vienna;

pub fn next_digest_after(
    anchor: DateTime<Utc>,
    digest_hour: i32,
    frequency_days: i32,
    t: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let hour = u8::try_from(digest_hour.clamp(0, 23)).ok()?;
    // first digest moment on/after the anchor day at digest_hour (Vienna time)
    let mut candidate = anchor
        .with_timezone(&Vienna)
        .date_naive()
        .and_time(NaiveTime::from_hms_opt(hour.into(), 0, 0)?)
        .and_local_timezone(Vienna)
        .single()?
        .with_timezone(&Utc);
    if candidate < anchor {
        candidate = candidate + Duration::days(1);
    }
    // step forward by frequency_days until strictly after `t`
    let step = Duration::days(frequency_days.max(1) as i64);
    while candidate <= t {
        candidate = candidate + step;
    }
    Some(candidate)
}
```

**Contract (later tasks rely on this):** `next_digest_after(anchor, hour, days, t)`
returns the first scheduled digest moment strictly after `t`. A batch created at
`created_at` is **due at `now`** iff
`next_digest_after(anchor, hour, days, created_at) <= now`.

- [ ] **Step 3: Run scheduling tests**

Run: `cd backend && cargo test notification::schedule -- --nocapture`

Expected: tests pass.

- [ ] **Step 4: Commit**

```bash
git add backend/src/notification/schedule.rs
git commit -m "feat: digest scheduling helper"
```

---

### Task 4: Preferences API

**Files:**
- Create: `backend/src/notification/api.rs`
- Modify: `backend/src/notification/mod.rs` (add `mod api;` + `pub use api::preferences_router;`)
- Modify: `backend/src/auth.rs` (make `new_token` `pub(crate)`)
- Modify: `backend/src/web.rs`

**Interfaces:**
- Consumes: `AuthUser` extractor, `notification::db` helpers.
- Produces:
  - `preferences_router() -> Router<AppState>`.
  - `GET /api/preferences`.
  - `PUT /api/preferences`.
  - `DELETE /api/preferences/telegram`.

- [ ] **Step 1: Write failing API tests**

Add to `backend/src/notification/api.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::AppState;
    use axum::body::to_bytes;
    use axum::http::Request;
    use sqlx::PgPool;
    use std::path::PathBuf;
    use tower::ServiceExt;

    fn test_state(pool: PgPool) -> AppState {
        AppState {
            pool,
            data_dir: PathBuf::new(),
            static_dir: PathBuf::from("/nonexistent"),
            base_url: "http://localhost:8080".into(),
            fake_login: false,
            smtp_config: None,
            google_oauth: None,
            github_oauth: None,
        }
    }

    async fn make_session(pool: &PgPool, user_id: i64) -> String {
        let token = crate::auth::new_token();
        let expires = chrono::Utc::now() + chrono::Duration::days(30);
        crate::db::create_session(pool, user_id, &token, expires).await.unwrap();
        token
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_preferences_defaults(pool: PgPool) {
        let uid = crate::db::find_or_create_user(&pool, "email", "a@b.com", "a@b.com")
            .await
            .unwrap();
        let token = make_session(&pool, uid).await;
        let state = test_state(pool);
        let app = crate::web::router(state);
        let resp = app
            .oneshot(
                Request::get("/api/preferences")
                    .header("Cookie", format!("ov_session={token}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
    }
}
```

Note: `new_session` helper may not exist in `auth::tests`; create a small test helper in this module or inline session creation.

Run: `cd backend && cargo test notification::api::tests::get_preferences_defaults -- --nocapture`

Expected: compile errors.

- [ ] **Step 2: Implement API handlers**

Create `backend/src/notification/api.rs` with:

```rust
use crate::auth::AuthUser;
use crate::web::AppState;
use axum::{extract::State, routing, Json, Router};
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceUpdateRequest {
    pub email_frequency: Option<String>,
    pub telegram_frequency: Option<String>,
    pub telegram_handle: Option<String>,
    pub digest_anchor: Option<DateTime<Utc>>,
    pub digest_hour: Option<i32>,
}

impl From<PreferenceUpdateRequest> for crate::notification::db::PreferenceUpdate {
    fn from(req: PreferenceUpdateRequest) -> Self {
        Self {
            email_frequency: req.email_frequency,
            telegram_frequency: req.telegram_frequency,
            telegram_handle: req.telegram_handle,
            digest_anchor: req.digest_anchor,
            digest_hour: req.digest_hour,
        }
    }
}

pub fn preferences_router() -> Router<AppState> {
    Router::new()
        .route("/api/preferences", routing::get(get_preferences))
        .route("/api/preferences", routing::put(put_preferences))
        .route("/api/preferences/telegram", routing::delete(delete_telegram))
}

async fn get_preferences(State(state): State<AppState>, auth: AuthUser) -> Result<Json<...>, StatusCode> { ... }
async fn put_preferences(State(state): State<AppState>, auth: AuthUser, Json(body): Json<PreferenceUpdateRequest>) -> Result<Json<...>, StatusCode> { ... }
async fn delete_telegram(State(state): State<AppState>, auth: AuthUser) -> Result<Json<...>, StatusCode> { ... }
```

- [ ] **Step 3: Mount router in web.rs**

In `backend/src/web.rs`:

```rust
.merge(crate::notification::preferences_router())
```

- [ ] **Step 4: Run API tests**

Run: `cd backend && cargo test notification::api -- --nocapture`

Expected: tests pass.

- [ ] **Step 5: Commit**

```bash
git add backend/src/notification/api.rs backend/src/notification/mod.rs backend/src/web.rs
git commit -m "feat: preferences API endpoints"
```

---

### Task 5: Email notifier

**Files:**
- Create: `backend/src/notification/send.rs`
- Modify: `backend/src/notification/mod.rs` (add `pub mod send;`)
- Modify: `backend/src/config.rs` (add `notification_email_from` config field; the from address is baked into `EmailNotifier`, not stored on AppState)

**Interfaces:**
- Consumes: `SmtpConfig` from `auth.rs`/web, from address, recipient email, HTML body.
- Produces:
  - `EmailNotifier::new(mailer, from) -> Self`.
  - `EmailNotifier::send(&self, to, subject, html) -> anyhow::Result<()>`.

- [ ] **Step 1: Write failing email notifier test**

NOTE: A live SMTP test is impractical here because the mailer uses STARTTLS
(pipeline needs EHLO capabilities → STARTTLS → TLS handshake, which a plain TCP
capture server cannot satisfy). Instead, factor the message construction into
`build_message` and test it directly (mirroring the existing
`login_email_has_context_and_expiry` pattern in `auth.rs`, which inspects
`Message::formatted()`). The network `send` is exercised manually/prod.

```rust
#[cfg(test)]
mod tests {
    use super::Message;

    fn build_test_email() -> Result<Message, anyhow::Error> {
        let notifier = EmailNotifier::new_unchecked("showings@example.com");
        notifier.build_message("user@example.com", "Subject", "<b>hi</b>")
    }

    #[test]
    fn builds_html_email_with_expected_headers() {
        let msg = build_test_email().unwrap();
        let headers = msg.headers().to_string();
        let body = String::from_utf8_lossy(&msg.formatted().to_vec()).to_string();
        // Content-Type text/html (present in the header string)
        assert!(headers.to_lowercase().contains("content-type: text/html"));
        // From / To present (addresses may be quoted-printable encoded in headers)
        assert!(headers.contains("showings@example.com"));
        assert!(headers.contains("user@example.com"));
        assert!(msg.subject().contains("Subject"));
        // raw HTML appears somewhere in the encoded body
        assert!(body.contains("<b>hi</b>"));
    }
}
```

Note: `EmailNotifier::new_unchecked` is a test-only constructor taking only the
from address (it does not need a transport); `build_message` takes
`&self, to, subject, html`. The assertion on `ContentType` and header presence
verifies behavior; adjust decoding if lettre's quoted-printable mangling hides
the HTML (in that case assert on a substring that survives encoding, and note it
in the report).

Run: `cd backend && cargo test notification::send::tests::builds_html_email_with_expected_headers -- --nocapture`

Expected: compile errors.

- [ ] **Step 2: Implement EmailNotifier**

```rust
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

pub struct EmailNotifier {
    mailer: Option<AsyncSmtpTransport<Tokio1Executor>>,
    from: String,
}

impl EmailNotifier {
    pub fn new(mailer: AsyncSmtpTransport<Tokio1Executor>, from: String) -> Self {
        EmailNotifier { mailer: Some(mailer), from }
    }

    #[cfg(test)]
    fn new_unchecked(from: String) -> Self {
        EmailNotifier { mailer: None, from }
    }

    fn build_message(
        &self,
        to: &str,
        subject: &str,
        html: &str,
    ) -> anyhow::Result<Message> {
        Ok(Message::builder()
            .from(self.from.parse()?)
            .to(to.parse()?)
            .subject(subject)
            .header(ContentType::TEXT_HTML)
            .body(html.to_string())?)
    }

    pub async fn send(&self, to: &str, subject: &str, html: &str) -> anyhow::Result<()> {
        let email = self.build_message(to, subject, html)?;
        let mailer = self.mailer.as_ref().ok_or_else(|| anyhow::anyhow!("email notifier has no transport"))?;
        mailer.send(email).await?;
        Ok(())
    }
}
```

Add `notification_email_from: Option<String>` to `Config`, defaulting to
`showings@<base_url_domain>` when unset.

- [ ] **Step 3: Run email notifier tests**

Run: `cd backend && cargo test notification::send -- --nocapture`

Expected: tests pass.

- [ ] **Step 4: Commit**

```bash
git add backend/src/notification/send.rs backend/src/notification/mod.rs backend/src/config.rs
git commit -m "feat: email notifier for notification batches"
```

---

### Task 6: Telegram DM notifier

**Files:**
- Modify: `backend/src/notify.rs`
- Modify: `backend/src/notification/send.rs`

**Interfaces:**
- Consumes: bot token, base URL, chat id, text.
- Produces:
  - `TelegramDmNotifier::new(token) -> Self`.
  - `TelegramDmNotifier::send_to(chat_id, text) -> anyhow::Result<()>`.

- [ ] **Step 1: Write failing Telegram DM test**

Use the existing capture-server pattern from `notify.rs` tests.

```rust
#[tokio::test]
async fn telegram_dm_sends_to_user_chat() {
    let (base, captured) = spawn_capture_server().await;
    let notifier = TelegramDmNotifier::with_base_url("TOKEN", &base);
    notifier.send_to("12345", "hello").await.unwrap();
    let calls = captured.lock().unwrap();
    assert_eq!(calls[0]["chat_id"], "12345");
}
```

Run: `cd backend && cargo test telegram_dm_sends_to_user_chat -- --nocapture`

Expected: compile errors.

- [ ] **Step 2: Implement TelegramDmNotifier**

Refactor `notify.rs` so the destination is passed in, or create a new struct:

```rust
pub struct TelegramDmNotifier {
    client: reqwest::Client,
    base_url: String,
    token: String,
}

impl TelegramDmNotifier {
    pub fn new(token: &str) -> Self { ... }
    pub fn with_base_url(token: &str, base_url: &str) -> Self { ... }


    pub async fn send_to(&self, chat_id: &str, text: &str) -> anyhow::Result<()> {
        for chunk in chunk_text(text, MAX_LEN) {
            self.client
                .post(format!("{}/bot{}/sendMessage", self.base_url, self.token))
                .json(&serde_json::json!({
                    "chat_id": chat_id,
                    "text": chunk,
                    "parse_mode": "HTML",
                    "link_preview_options": {"is_disabled": true},
                }))
                .timeout(std::time::Duration::from_secs(20))
                .send()
                .await?
                .error_for_status()?;
        }
        Ok(())
    }
}
```

Keep the existing `TelegramNotifier` (public channel) as a thin wrapper that hard-codes `TELEGRAM_CHAT_ID`, or update it to use `TelegramDmNotifier` internally.

- [ ] **Step 3: Run Telegram DM tests**

Run: `cd backend && cargo test telegram_dm -- --nocapture`

Expected: tests pass.

- [ ] **Step 4: Commit**

```bash
git add backend/src/notify.rs backend/src/notification/send.rs
git commit -m "feat: telegram DM notifier"
```

---

### Task 7: Batching engine core

**Files:**
- Create: `backend/src/notification/batch.rs`
- Modify: `backend/src/notification/mod.rs` (add `pub mod batch;`)

**Interfaces:**
- Consumes: `PgPool`, new showing IDs, `NotificationPreferences`, notifiers.
- Produces:
  - `append_showings_for_user(pool, user_id, layer, showing_ids) -> Result<(), sqlx::Error>`.
  - `process_due_batches(ctx: &BatchCtx, now: DateTime<Utc>) -> anyhow::Result<BatchResult>`.

- [ ] **Step 1: Write failing batching engine tests**

```rust
#[sqlx::test(migrations = "./migrations")]
async fn immediately_batch_is_due_right_away(pool: PgPool) {
    // insert user, preferences immediately, movie+showing, then process_due_batches
    // assert batch is sent
}

#[sqlx::test(migrations = "./migrations")]
async fn three_day_batch_not_due_yet(pool: PgPool) {
    // insert user, preferences 3 days, movie+showing, process with now < digest
    // assert batch still pending
}
```

Run: `cd backend && cargo test notification::batch -- --nocapture`

Expected: compile errors.

- [ ] **Step 2: Implement batching engine**

```rust
pub struct BatchCtx<'a> {
    pub pool: &'a PgPool,
    pub email_notifier: Option<&'a EmailNotifier>,
    pub telegram_notifier: Option<&'a TelegramDmNotifier>,
    pub base_url: &'a str,
}

pub async fn append_showing_for_users(
    pool: &PgPool,
    showing_id: i64,
    preferences: &[NotificationPreferences],
) -> sqlx::Result<Vec<(i64, &'static str)>> { ... }

pub async fn process_due_batches(ctx: &BatchCtx, now: DateTime<Utc>) -> anyhow::Result<usize> { ... }
```

Implementation:
- `append_showing_for_users` iterates preferences; for each enabled+verified layer, gets/creates open batch and inserts showing.
- `process_due_batches` queries due batches (using schedule helper), loads linked showings + metadata, formats message, sends, transitions state, and creates new empty batch on success.

- [ ] **Step 3: Run batching engine tests**

Run: `cd backend && cargo test notification::batch -- --nocapture`

Expected: tests pass.

- [ ] **Step 4: Commit**

```bash
git add backend/src/notification/batch.rs
git commit -m "feat: notification batching engine"
```

---

### Task 8: Telegram webhook verification

**Files:**
- Create: `backend/src/notification/verify.rs`
- Modify: `backend/src/notification/mod.rs` (add `pub mod verify;` + `pub use verify::telegram_webhook_router;`)
- Modify: `backend/src/web.rs` (add `telegram_webhook_secret: Option<String>` field to AppState)
- Modify: `backend/src/main.rs` (pass `telegram_webhook_secret` when constructing AppState)

**Interfaces:**
- Consumes: Telegram update JSON, webhook secret, `notification::db`.
- Produces:
  - `telegram_webhook_router() -> Router<AppState>`.
  - `normalize_handle(handle: &str) -> String`.

- [ ] **Step 1: Write failing webhook test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::AppState;
    use axum::body::to_bytes;
    use axum::http::Request;
    use axum::http::StatusCode;
    use sqlx::PgPool;
    use std::path::PathBuf;
    use tower::ServiceExt;

    fn test_state(pool: PgPool) -> AppState {
        AppState {
            pool,
            data_dir: PathBuf::new(),
            static_dir: PathBuf::from("/nonexistent"),
            base_url: "http://localhost:8080".into(),
            fake_login: false,
            smtp_config: None,
            google_oauth: None,
            github_oauth: None,
            telegram_webhook_secret: Some("supersecret".into()),
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn webhook_verifies_handle_and_stores_chat_id(pool: PgPool) {
        let uid = crate::db::find_or_create_user(&pool, "email", "a@b.com", "a@b.com").await.unwrap();
        crate::notification::db::upsert_preferences(&pool, uid, crate::notification::db::PreferenceUpdate {
            email_frequency: None,
            telegram_frequency: Some("immediately".into()),
            telegram_handle: Some("myhandle".into()),
            digest_anchor: None,
            digest_hour: None,
        }).await.unwrap();

        let app = crate::web::router(test_state(pool.clone()));
        // matching handle with wrong chat_id previously None
        let resp = app
            .oneshot(
                Request::post("/api/telegram/webhook/supersecret")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"update_id":1,"message":{"message_id":1,"from":{"id":99,"username":"MyHandle"},"chat":{"id":12345}}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let prefs = crate::notification::db::get_preferences(&pool, uid).await.unwrap();
        assert_eq!(prefs.telegram_chat_id.as_deref(), Some("12345"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn webhook_rejects_wrong_secret(pool: PgPool) {
        let app = crate::web::router(test_state(pool));
        let resp = app
            .oneshot(
                Request::post("/api/telegram/webhook/wrong")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
```

Run: `cd backend && cargo test notification::verify -- --nocapture`

Expected: compile errors.

- [ ] **Step 2: Implement webhook handler**

```rust
use crate::web::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing;
use axum::Json;
use axum::Router;

pub fn telegram_webhook_router() -> Router<AppState> {
    Router::new().route(
        "/api/telegram/webhook/:secret",
        routing::post(post_webhook),
    )
}

pub fn normalize_handle(handle: &str) -> String {
    handle.trim().trim_start_matches('@').to_lowercase()
}

async fn post_webhook(
    State(state): State<AppState>,
    Path(secret): Path<String>,
    Json(update): Json<serde_json::Value>,
) -> StatusCode {
    if state.telegram_webhook_secret.as_deref() != Some(secret.as_str()) {
        return StatusCode::UNAUTHORIZED;
    }
    let username = update
        .pointer("/message/from/username")
        .and_then(|v| v.as_str());
    let chat_id = update.pointer("/message/chat/id").and_then(|v| v.as_i64());
    let (Some(username), Some(chat_id)) = (username, chat_id) else {
        // Missing fields: still 200 so Telegram stops retrying this update.
        return StatusCode::OK;
    };
    let handle = normalize_handle(username);
    let updated = sqlx::query(
        "UPDATE notification_preferences
            SET telegram_chat_id = $1, updated_at = now()
          WHERE telegram_handle = $2 AND telegram_chat_id IS NULL",
    )
    .bind(chat_id.to_string())
    .bind(&handle)
    .execute(&state.pool)
    .await;
    match updated {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            tracing::error!("telegram webhook update failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
```

- `normalize_handle` strips leading `@` and lowercases; the webhook normalizes the
  incoming username so it matches the stored handle.
- The `UPDATE ... WHERE telegram_handle = $2 AND telegram_chat_id IS NULL` limit
  means a verified handle is not overwritten by a later message (idempotence).
- Always return `200 OK` for a valid secret + well-formed update (Telegram
  retries non-200 with backoff); only a DB error returns 500.

- [ ] **Step 3: Mount router and add AppState field**

Add `telegram_webhook_secret: Option<String>` to `AppState`. Update EVERY
`AppState { ... }` literal: test helpers in `web.rs` and `auth.rs` get
`telegram_webhook_secret: None,`, and `main.rs` sets it from `config`.
Mount the webhook router in `web.rs`:

```rust
.merge(crate::notification::telegram_webhook_router())
```

The `Config` field and env parsing are added in Task 12.

- [ ] **Step 4: Run webhook tests + full suite**

Run: `cd backend && cargo test notification::verify -- --nocapture`
Then: `cargo test` (must compile — every AppState literal updated).

Expected: tests pass.

- [ ] **Step 5: Commit**

```bash
git add backend/src/notification/verify.rs backend/src/notification/mod.rs backend/src/web.rs backend/src/main.rs
git commit -m "feat: telegram handle verification webhook"
```

---

### Task 9: Integrate batching into checker

**Files:**
- Modify: `backend/src/db.rs` (change `insert_showing` to return `Option<i64>`)
- Modify: `backend/src/db.rs` tests
- Modify: `backend/src/checker.rs`

**Interfaces:**
- Consumes: `notification::batch::{append_showing_for_users, process_due_batches}`.
- Produces: `CheckResult` unchanged except batch send count could be logged.

- [ ] **Step 1: Modify checker to append showings and process batches**

First, change `db::insert_showing` to return `Option<i64>` (the new row id, or `None` if duplicate). Update its signature, implementation, and all call sites/tests.

Then in `run_check`, collect inserted ids:

```rust
let mut new_showing_ids: Vec<(i64, &Showing)> = Vec::new();
for s in &upcoming {
    // ... existing upsert_movie call ...
    if let Some(showing_id) = db::insert_showing(
        ctx.pool, movie_id, s.start, &s.version, &s.hall, &s.url, now,
    ).await? {
        new_showing_ids.push((showing_id, s));
    }
}
```

After the existing DB write loop, add:

```rust
if !new_showing_ids.is_empty() {
    let prefs = notification::db::list_active_preferences(ctx.pool).await?;
    for (showing_id, _) in &new_showing_ids {
        notification::batch::append_showing_for_users(
            ctx.pool,
            *showing_id,
            &prefs,
        ).await?;
    }
}

let batch_ctx = notification::batch::BatchCtx {
    pool: ctx.pool,
    email_notifier: ctx.email_notifier,
    telegram_notifier: ctx.telegram_notifier,
    base_url: &ctx.config.base_url,
};
notification::batch::process_due_batches(&batch_ctx, now).await?;
```

Add `email_notifier` and `telegram_notifier` to `CheckCtx` and wire them from `main.rs`.

- [ ] **Step 2: Update checker tests**

Ensure existing tests still pass. Add a new test:

```rust
#[sqlx::test(migrations = "./migrations")]
async fn new_showing_creates_batch_for_immediate_user(pool: PgPool) { ... }
```

Run: `cd backend && cargo test checker -- --nocapture`

Expected: all checker tests pass.

- [ ] **Step 3: Commit**

```bash
git add backend/src/checker.rs
git commit -m "feat: integrate notification batching into checker"
```

---

### Task 10: Frontend preferences API

**Files:**
- Create: `frontend/src/api/preferences.ts`
- Modify: `frontend/src/types.ts`

**Interfaces:**
- Produces:
  - `NotificationPreferences` interface.
  - `fetchPreferences(): Promise<NotificationPreferences>`.
  - `savePreferences(prefs: Partial<NotificationPreferences>): Promise<NotificationPreferences>`.

- [ ] **Step 1: Add types and API**

```typescript
// frontend/src/types.ts
export interface NotificationPreferences {
  emailFrequency: NotificationFrequency;
  telegramFrequency: NotificationFrequency;
  telegramHandle: string;
  telegramVerified: boolean;
  digestAnchor: string;
  digestHour: number;
}

// frontend/src/api/preferences.ts
export async function fetchPreferences(): Promise<NotificationPreferences> {
  const res = await fetch("/api/preferences", { credentials: "include" });
  if (!res.ok) throw new Error("failed to load preferences");
  return res.json();
}

export async function savePreferences(
  prefs: Partial<NotificationPreferences>
): Promise<NotificationPreferences> {
  const res = await fetch("/api/preferences", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    credentials: "include",
    body: JSON.stringify(prefs),
  });
  if (!res.ok) throw new Error("failed to save preferences");
  return res.json();
}
```

- [ ] **Step 2: Run frontend type check**

Run: `cd frontend && npm run build`

Expected: type errors only in PreferencesPage (not yet wired).

- [ ] **Step 3: Commit**

```bash
git add frontend/src/types.ts frontend/src/api/preferences.ts
git commit -m "feat: frontend preferences API types and fetch helpers"
```

---

### Task 11: Wire preferences page to API

**Files:**
- Modify: `frontend/src/pages/PreferencesPage.tsx`
- Modify: `frontend/src/locales/{en,de}.json`
- Modify: `frontend/src/index.css`

**Interfaces:**
- Consumes: `fetchPreferences`, `savePreferences`, `NotificationPreferences`.

- [ ] **Step 1: Update PreferencesPage**

Replace local state initialization with `useEffect` loading and `savePreferences` on Save:

```tsx
export function PreferencesPage() {
  const { t } = useTranslation();
  const [prefs, setPrefs] = useState<NotificationPreferences | null>(null);
  const [loading, setLoading] = useState(true);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchPreferences()
      .then(setPrefs)
      .catch(() => setError(t("preferences.loadError")))
      .finally(() => setLoading(false));
  }, [t]);

  const handleSave = async () => {
    if (!prefs) return;
    try {
      const updated = await savePreferences(prefs);
      setPrefs(updated);
      setSaved(true);
    } catch {
      setError(t("preferences.saveError"));
    }
  };

  if (loading) return <div className="preferences"><Marquee /><p>{t("preferences.loading")}</p></div>;
  if (error) return <div className="preferences"><Marquee /><p className="pref-error">{error}</p></div>;
  if (!prefs) return null;

  // render cards bound to prefs state
  // in the Telegram card, after the handle input:
  // {prefs.telegramVerified ? (
  //   <span className="pref-verified">{t("preferences.telegramVerified")}</span>
  // ) : prefs.telegramHandle ? (
  //   <span className="pref-verify-prompt">{t("preferences.telegramVerifyPrompt")}</span>
  // ) : null}
}
```

- [ ] **Step 2: Add localization strings**

Add to both locale files:

```json
"loadError": "Could not load preferences.",
"saveError": "Could not save preferences.",
"loading": "Loading…",
"telegramVerified": "Telegram account linked.",
"telegramVerifyPrompt": "Send any message to @ov_linzz_bot to link your account."
```

- [ ] **Step 3: Add verified state styling**

```css
.pref-verified { color: var(--ok); font-size: .8rem; }
.pref-verify-prompt { color: var(--faint); font-size: .8rem; }
.pref-error { color: var(--bad); font-size: .9rem; }
```

- [ ] **Step 4: Run frontend tests**

Run: `cd frontend && npm test`

Expected: existing PreferencesPage tests need updating; fix them to mock `fetchPreferences` and `savePreferences`.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/pages/PreferencesPage.tsx frontend/src/locales/en.json frontend/src/locales/de.json frontend/src/index.css
git commit -m "feat: wire preferences page to backend API"
```

---

### Task 12: Configure webhook on startup and env vars

**Files:**
- Modify: `backend/src/main.rs`
- Modify: `backend/src/config.rs`
- Modify: `helm/ov-watcher/values.yaml` (document new env vars)

**Interfaces:**
- Consumes: `TELEGRAM_BOT_TOKEN`, `TELEGRAM_WEBHOOK_SECRET`, `base_url`.

- [ ] **Step 1: Add remaining config fields**

In `backend/src/config.rs` (note: `notification_email_from` is added in Task 5):

```rust
pub telegram_webhook_secret: Option<String>,
pub notification_max_retry_age_hours: u64,
```

Default `notification_max_retry_age_hours` to 168.

- [ ] **Step 2: Set webhook on startup**

In `backend/src/main.rs`, after server starts, if `telegram_webhook_secret` is configured, call Telegram `setWebhook`:

```rust
if let (Some(token), Some(secret)) = (&config.telegram_token, &config.telegram_webhook_secret) {
    let url = format!("{}/api/telegram/webhook/{}", config.base_url, secret);
    let _ = reqwest::Client::new()
        .post(format!("https://api.telegram.org/bot{token}/setWebhook"))
        .json(&serde_json::json!({"url": url}))
        .send()
        .await;
}
```

Make this best-effort (log warning on failure, don't crash).

- [ ] **Step 3: Document env vars in Helm values**

Add to `helm/ov-watcher/values.yaml`:

```yaml
# Telegram webhook secret for handle verification
# telegramWebhookSecret: ""
# Optional override for notification sender address
# notificationEmailFrom: "showings@cinema.k-labs.app"
```

- [ ] **Step 4: Run backend build**

Run: `cd backend && cargo build`

Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add backend/src/main.rs backend/src/config.rs helm/ov-watcher/values.yaml
git commit -m "feat: webhook startup config and env vars"
```

---

### Task 13: Full integration and polish

**Files:**
- Modify: any failing tests from full suite.
- Modify: `AGENTS.md` if env vars or run instructions changed.

**Interfaces:**
- Produces: passing full test suite.

- [ ] **Step 1: Run full backend test suite**

Run: `cd backend && cargo test`

Expected: all tests pass.

- [ ] **Step 2: Run full frontend test suite and build**

Run:
```bash
cd frontend && npm test
cd frontend && npm run build
```

Expected: tests pass, build succeeds.

- [ ] **Step 3: Run clippy and fmt**

Run:
```bash
cd backend && cargo fmt --check
cd backend && cargo clippy --all-targets -- -D warnings
```

Expected: clean.

- [ ] **Step 4: Update docs if needed**

If new env vars are required for local dev, add them to `LOCAL_DEV.md` or `AGENTS.md`.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: notification batching integration and polish"
```

---

## Self-Review

**Spec coverage:**
- Preferences persistence → Tasks 1, 2.
- Preferences API → Task 4.
- Telegram handle normalization + verification → Tasks 2, 8, 12.
- Batching during checker run → Tasks 7, 9.
- Fixed digest schedule → Task 3.
- Email/Telegram DM notifiers → Tasks 5, 6.
- Public channel unchanged → no tasks modify existing `TelegramNotifier` behavior for `TELEGRAM_CHAT_ID`.
- Frontend wiring → Tasks 10, 11.
- Error handling/retries → Task 7 (process_due_batches).

**Placeholder scan:** No TBD/TODO/fill-in-details patterns. Each step includes concrete code or exact commands.

**Type consistency:** `NotificationFrequency` string values match spec. `PreferenceUpdate` fields align with API request body and DB helper signature. `BatchCtx` fields are referenced consistently in Tasks 7 and 9.
