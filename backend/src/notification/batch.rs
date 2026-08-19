use crate::models::{MovieMeta, Showing};
use crate::notification::db::{self, DueBatch, NotificationPreferences};
use crate::notification::schedule::{next_digest_after, parse_frequency, Frequency};
use crate::notification::send::EmailNotifier;
use crate::notify::{self, TelegramDmNotifier};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::collections::HashMap;

const EMAIL_SUBJECT: &str = "Neue OV-Vorstellungen in Linz";

#[async_trait::async_trait]
pub trait EmailSender: Send + Sync {
    async fn send_email(&self, to: &str, subject: &str, html: &str) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
impl EmailSender for EmailNotifier {
    async fn send_email(&self, to: &str, subject: &str, html: &str) -> anyhow::Result<()> {
        self.send(to, subject, html).await
    }
}

#[async_trait::async_trait]
pub trait TelegramSender: Send + Sync {
    async fn send_telegram(&self, chat_id: &str, text: &str) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
impl TelegramSender for TelegramDmNotifier {
    async fn send_telegram(&self, chat_id: &str, text: &str) -> anyhow::Result<()> {
        self.send_to(chat_id, text).await
    }
}

pub struct BatchCtx<'a> {
    pub pool: &'a PgPool,
    pub email: Option<&'a dyn EmailSender>,
    pub telegram: Option<&'a dyn TelegramSender>,
    pub base_url: &'a str,
    pub max_retry_age_hours: u64,
}

pub async fn append_showing_for_users(
    pool: &PgPool,
    showing_id: i64,
    preferences: &[NotificationPreferences],
) -> sqlx::Result<Vec<(i64, String)>> {
    let mut affected: Vec<(i64, String)> = Vec::new();
    for prefs in preferences {
        if prefs.email_frequency != "never" {
            let batch_id = db::get_or_create_open_batch(pool, prefs.user_id, "email").await?;
            db::append_showing_to_batch(pool, batch_id, showing_id).await?;
            affected.push((prefs.user_id, "email".to_string()));
        }
        if prefs.telegram_frequency != "never" && prefs.telegram_chat_id.is_some() {
            let batch_id = db::get_or_create_open_batch(pool, prefs.user_id, "telegram").await?;
            db::append_showing_to_batch(pool, batch_id, showing_id).await?;
            affected.push((prefs.user_id, "telegram".to_string()));
        }
    }
    Ok(affected)
}

pub async fn process_due_batches(ctx: &BatchCtx<'_>, now: DateTime<Utc>) -> anyhow::Result<usize> {
    db::gc_failed_batches(ctx.pool, ctx.max_retry_age_hours, now).await?;
    let batches = db::get_due_batches(ctx.pool, now).await?;
    let mut sent = 0usize;
    for batch in batches {
        if !batch_is_due(&batch, now) {
            continue;
        }
        match handle_batch(ctx, &batch).await {
            Ok(Outcome::Sent) => sent += 1,
            Ok(Outcome::Skipped) => {}
            Err(e) => {
                tracing::warn!(batch_id = batch.batch_id, error = %e, "notification batch send failed");
                if let Err(mark_err) =
                    db::mark_batch_failed(ctx.pool, batch.batch_id, &e.to_string()).await
                {
                    tracing::warn!(
                        batch_id = batch.batch_id,
                        error = %mark_err,
                        "failed to mark batch failed"
                    );
                }
            }
        }
    }
    Ok(sent)
}

fn batch_is_due(batch: &DueBatch, now: DateTime<Utc>) -> bool {
    match parse_frequency(&batch.frequency) {
        Some(Frequency::Immediately) => true,
        Some(Frequency::Days(n)) => {
            next_digest_after(batch.digest_anchor, batch.digest_hour, n, batch.created_at)
                .is_some_and(|digest| digest <= now)
        }
        _ => false,
    }
}

enum Outcome {
    Sent,
    Skipped,
}

async fn handle_batch(ctx: &BatchCtx<'_>, batch: &DueBatch) -> anyhow::Result<Outcome> {
    let (showings, metas) = load_batch_showings(ctx.pool, batch.batch_id).await?;
    let ignored = crate::db::ignored_keys(ctx.pool, batch.user_id).await?;
    let showings: Vec<_> = showings
        .into_iter()
        .filter(|s| !ignored.contains(&(s.cinema.clone(), s.movie.clone())))
        .collect();
    let metas: HashMap<String, MovieMeta> = metas
        .into_iter()
        .filter(|(k, _)| {
            let (cinema, movie) = k.split_once('|').unwrap_or((k, ""));
            !ignored.contains(&(cinema.to_string(), movie.to_string()))
        })
        .collect();
    if showings.is_empty() {
        return Ok(Outcome::Skipped);
    }
    let body = notify::format_message(&showings, &metas);
    match batch.layer.as_str() {
        "email" => {
            let Some(sender) = ctx.email else {
                tracing::warn!(
                    batch_id = batch.batch_id,
                    "email batch due but email sender not configured"
                );
                return Ok(Outcome::Skipped);
            };
            let Some(to) = user_email(ctx.pool, batch.user_id).await? else {
                tracing::warn!(
                    user_id = batch.user_id,
                    "email batch due but user has no email"
                );
                return Ok(Outcome::Skipped);
            };
            db::mark_batch_sending(ctx.pool, batch.batch_id).await?;
            sender
                .send_email(&to, EMAIL_SUBJECT, &wrap_email_html(&body, ctx.base_url))
                .await?;
        }
        "telegram" => {
            let Some(sender) = ctx.telegram else {
                tracing::warn!(
                    batch_id = batch.batch_id,
                    "telegram batch due but telegram sender not configured"
                );
                return Ok(Outcome::Skipped);
            };
            let Some(chat_id) = telegram_chat_id(ctx.pool, batch.user_id).await? else {
                tracing::warn!(
                    user_id = batch.user_id,
                    "telegram batch due but chat id missing"
                );
                return Ok(Outcome::Skipped);
            };
            db::mark_batch_sending(ctx.pool, batch.batch_id).await?;
            sender.send_telegram(&chat_id, &body).await?;
        }
        layer => {
            tracing::warn!(batch_id = batch.batch_id, %layer, "unknown batch layer");
            return Ok(Outcome::Skipped);
        }
    }
    db::mark_batch_sent(ctx.pool, batch.batch_id).await?;
    if let Err(e) = db::create_empty_batch(ctx.pool, batch.user_id, &batch.layer).await {
        tracing::warn!(batch_id = batch.batch_id, error = %e, "failed to create next empty batch");
    }
    Ok(Outcome::Sent)
}

#[derive(Debug, sqlx::FromRow)]
struct BatchShowingRow {
    cinema: String,
    movie: String,
    start: DateTime<Utc>,
    version: String,
    hall: String,
    url: String,
    runtime_min: Option<i32>,
    genres: Vec<String>,
}

async fn load_batch_showings(
    pool: &PgPool,
    batch_id: i64,
) -> sqlx::Result<(Vec<Showing>, HashMap<String, MovieMeta>)> {
    let rows: Vec<BatchShowingRow> = sqlx::query_as(
        "SELECT m.cinema, m.title AS movie, s.start, s.version, s.hall, s.url,
                m.runtime_min, m.genres
         FROM notification_batch_showing nbs
         JOIN showing s ON s.id = nbs.showing_id
         JOIN movie m ON m.id = s.movie_id
         WHERE nbs.batch_id = $1
         ORDER BY s.start",
    )
    .bind(batch_id)
    .fetch_all(pool)
    .await?;
    let showings: Vec<Showing> = rows
        .iter()
        .map(|r| Showing {
            cinema: r.cinema.clone(),
            movie: r.movie.clone(),
            start: r.start,
            version: r.version.clone(),
            hall: r.hall.clone(),
            url: r.url.clone(),
        })
        .collect();
    let mut metas: HashMap<String, MovieMeta> = HashMap::new();
    for r in rows {
        metas.insert(
            format!("{}|{}", r.cinema, r.movie),
            MovieMeta {
                runtime_min: r.runtime_min,
                genres: r.genres.clone(),
                poster: None,
            },
        );
    }
    Ok((showings, metas))
}

async fn user_email(pool: &PgPool, user_id: i64) -> sqlx::Result<Option<String>> {
    let row: Option<(Option<String>,)> = sqlx::query_as("SELECT email FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.and_then(|(e,)| e).filter(|e| !e.is_empty()))
}

async fn telegram_chat_id(pool: &PgPool, user_id: i64) -> sqlx::Result<Option<String>> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT telegram_chat_id FROM notification_preferences WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|(c,)| c).filter(|c| !c.is_empty()))
}

fn wrap_email_html(body: &str, base_url: &str) -> String {
    let prefs_url = notify::escape_attr(&format!("{}/preferences", base_url.trim_end_matches('/')));
    format!(
        "<html><body style=\"font-family:sans-serif;line-height:1.5\">\n{}\n<p style=\"color:#888\"><a href=\"{prefs_url}\">Einstellungen ändern</a></p>\n</body></html>",
        body
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::notification::db::{get_preferences, upsert_preferences, PreferenceUpdate};
    use chrono::{TimeZone, Utc};
    use chrono_tz::Europe::Vienna;
    use sqlx::PgPool;
    use std::sync::{Arc, Mutex};

    fn at(day: u32, hour: u32) -> DateTime<Utc> {
        Vienna
            .with_ymd_and_hms(2026, 8, day, hour, 0, 0)
            .unwrap()
            .with_timezone(&Utc)
    }

    async fn make_user(pool: &PgPool, email: &str) -> i64 {
        db::find_or_create_user(pool, "email", email, email)
            .await
            .unwrap()
    }

    async fn make_showing(pool: &PgPool, title: &str) -> i64 {
        let mid = db::upsert_movie(
            pool,
            "Cineplexx Linz",
            title,
            Some(120),
            &["Drama".into()],
            None,
            None,
        )
        .await
        .unwrap();
        let inserted = db::insert_showing(
            pool,
            mid,
            at(20, 19),
            "OV",
            "Saal 6",
            "https://x",
            at(18, 10),
        )
        .await
        .unwrap();
        assert!(inserted.is_some());
        sqlx::query_as::<_, (i64,)>("SELECT id FROM showing WHERE movie_id = $1")
            .bind(mid)
            .fetch_one(pool)
            .await
            .unwrap()
            .0
    }

    async fn prefs_for(
        pool: &PgPool,
        uid: i64,
        email_freq: &str,
        telegram_freq: &str,
        handle: Option<&str>,
        digest_anchor: Option<DateTime<Utc>>,
        digest_hour: Option<i32>,
    ) {
        upsert_preferences(
            pool,
            uid,
            PreferenceUpdate {
                email_frequency: Some(email_freq.to_string()),
                telegram_frequency: Some(telegram_freq.to_string()),
                telegram_handle: handle.map(|s| s.to_string()),
                digest_anchor,
                digest_hour,
            },
        )
        .await
        .unwrap();
    }

    async fn set_chat_id(pool: &PgPool, uid: i64, chat_id: &str) {
        sqlx::query("UPDATE notification_preferences SET telegram_chat_id = $2 WHERE user_id = $1")
            .bind(uid)
            .bind(chat_id)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn set_batch_created_at(pool: &PgPool, batch_id: i64, created_at: DateTime<Utc>) {
        sqlx::query("UPDATE notification_batch SET created_at = $2 WHERE id = $1")
            .bind(batch_id)
            .bind(created_at)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn open_batch_id(pool: &PgPool, uid: i64, layer: &str) -> i64 {
        sqlx::query_as::<_, (i64,)>(
            "SELECT id FROM notification_batch
             WHERE user_id = $1 AND layer = $2 AND status = 'pending'",
        )
        .bind(uid)
        .bind(layer)
        .fetch_one(pool)
        .await
        .unwrap()
        .0
    }

    async fn batch_status(pool: &PgPool, batch_id: i64) -> String {
        sqlx::query_as::<_, (String,)>("SELECT status FROM notification_batch WHERE id = $1")
            .bind(batch_id)
            .fetch_one(pool)
            .await
            .unwrap()
            .0
    }

    async fn count_open_batches(pool: &PgPool, uid: i64, layer: &str) -> i64 {
        sqlx::query_as::<_, (i64,)>(
            "SELECT count(*) FROM notification_batch
             WHERE user_id = $1 AND layer = $2 AND status = 'pending'",
        )
        .bind(uid)
        .bind(layer)
        .fetch_one(pool)
        .await
        .unwrap()
        .0
    }

    fn ctx<'a>(
        pool: &'a PgPool,
        email: Option<&'a dyn EmailSender>,
        telegram: Option<&'a dyn TelegramSender>,
    ) -> BatchCtx<'a> {
        BatchCtx {
            pool,
            email,
            telegram,
            base_url: "https://cinema.k-labs.app",
            max_retry_age_hours: 168,
        }
    }

    #[derive(Default)]
    struct RecordingEmail {
        sent: Arc<Mutex<Vec<(String, String, String)>>>,
    }

    #[async_trait::async_trait]
    impl EmailSender for RecordingEmail {
        async fn send_email(&self, to: &str, subject: &str, html: &str) -> anyhow::Result<()> {
            self.sent
                .lock()
                .unwrap()
                .push((to.to_string(), subject.to_string(), html.to_string()));
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingTelegram {
        sent: Arc<Mutex<Vec<(String, String)>>>,
    }

    #[async_trait::async_trait]
    impl TelegramSender for RecordingTelegram {
        async fn send_telegram(&self, chat_id: &str, text: &str) -> anyhow::Result<()> {
            self.sent
                .lock()
                .unwrap()
                .push((chat_id.to_string(), text.to_string()));
            Ok(())
        }
    }

    struct FlakyTelegram {
        attempts: Arc<Mutex<usize>>,
        sent: Arc<Mutex<Vec<(String, String)>>>,
    }

    #[async_trait::async_trait]
    impl TelegramSender for FlakyTelegram {
        async fn send_telegram(&self, chat_id: &str, text: &str) -> anyhow::Result<()> {
            let mut attempts = self.attempts.lock().unwrap();
            *attempts += 1;
            if *attempts == 1 {
                Err(anyhow::anyhow!("network down"))
            } else {
                self.sent
                    .lock()
                    .unwrap()
                    .push((chat_id.to_string(), text.to_string()));
                Ok(())
            }
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn immediately_email_batch_is_sent(pool: PgPool) {
        let uid = make_user(&pool, "a@b.com").await;
        prefs_for(&pool, uid, "immediately", "never", None, None, None).await;
        let showing_id = make_showing(&pool, "The Odyssey").await;

        let affected = append_showing_for_users(
            &pool,
            showing_id,
            &[get_preferences(&pool, uid).await.unwrap()],
        )
        .await
        .unwrap();
        assert_eq!(affected, vec![(uid, "email".to_string())]);
        let batch_id = open_batch_id(&pool, uid, "email").await;
        assert_eq!(batch_status(&pool, batch_id).await, "pending");

        let email = RecordingEmail::default();
        let sent = process_due_batches(&ctx(&pool, Some(&email), None), at(18, 12))
            .await
            .unwrap();
        assert_eq!(sent, 1);

        {
            let sent_emails = email.sent.lock().unwrap();
            assert_eq!(sent_emails.len(), 1);
            assert_eq!(sent_emails[0].0, "a@b.com");
            assert_eq!(sent_emails[0].1, EMAIL_SUBJECT);
            assert!(sent_emails[0].2.contains("The Odyssey"));
            assert!(sent_emails[0]
                .2
                .contains("https://cinema.k-labs.app/preferences"));
        }

        assert_eq!(batch_status(&pool, batch_id).await, "sent");
        assert_eq!(count_open_batches(&pool, uid, "email").await, 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn three_day_telegram_batch_not_due_yet(pool: PgPool) {
        let uid = make_user(&pool, "c@d.com").await;
        prefs_for(
            &pool,
            uid,
            "never",
            "3",
            Some("myhandle"),
            Some(at(16, 9)),
            Some(9),
        )
        .await;
        set_chat_id(&pool, uid, "12345").await;
        let showing_id = make_showing(&pool, "F1").await;

        let affected = append_showing_for_users(
            &pool,
            showing_id,
            &[get_preferences(&pool, uid).await.unwrap()],
        )
        .await
        .unwrap();
        assert_eq!(affected, vec![(uid, "telegram".to_string())]);
        let batch_id = open_batch_id(&pool, uid, "telegram").await;
        set_batch_created_at(&pool, batch_id, at(19, 8)).await;

        let tg = RecordingTelegram::default();
        let sent = process_due_batches(&ctx(&pool, None, Some(&tg)), at(19, 8))
            .await
            .unwrap();
        assert_eq!(sent, 0);
        assert!(tg.sent.lock().unwrap().is_empty());
        assert_eq!(batch_status(&pool, batch_id).await, "pending");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn three_day_batch_due_after_digest(pool: PgPool) {
        let uid = make_user(&pool, "e@f.com").await;
        prefs_for(
            &pool,
            uid,
            "never",
            "3",
            Some("myhandle"),
            Some(at(16, 9)),
            Some(9),
        )
        .await;
        set_chat_id(&pool, uid, "12345").await;
        let showing_id = make_showing(&pool, "F1").await;
        append_showing_for_users(
            &pool,
            showing_id,
            &[get_preferences(&pool, uid).await.unwrap()],
        )
        .await
        .unwrap();
        let batch_id = open_batch_id(&pool, uid, "telegram").await;
        set_batch_created_at(&pool, batch_id, at(19, 8)).await;

        let tg = RecordingTelegram::default();
        let sent = process_due_batches(&ctx(&pool, None, Some(&tg)), at(19, 9))
            .await
            .unwrap();
        assert_eq!(sent, 1);
        {
            let sent_msgs = tg.sent.lock().unwrap();
            assert_eq!(sent_msgs.len(), 1);
            assert_eq!(sent_msgs[0].0, "12345");
            assert!(sent_msgs[0].1.contains("F1"));
        }
        assert_eq!(batch_status(&pool, batch_id).await, "sent");
        assert_eq!(count_open_batches(&pool, uid, "telegram").await, 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn unverified_telegram_handle_does_not_create_batch(pool: PgPool) {
        let uid = make_user(&pool, "g@h.com").await;
        prefs_for(
            &pool,
            uid,
            "never",
            "immediately",
            Some("nochat"),
            None,
            None,
        )
        .await;
        let showing_id = make_showing(&pool, "F1").await;

        let affected = append_showing_for_users(
            &pool,
            showing_id,
            &[get_preferences(&pool, uid).await.unwrap()],
        )
        .await
        .unwrap();
        assert!(affected.is_empty());
        let count: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM notification_batch WHERE user_id = $1 AND layer = 'telegram'",
        )
        .bind(uid)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count.0, 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn failed_batch_is_retried(pool: PgPool) {
        let uid = make_user(&pool, "i@j.com").await;
        prefs_for(
            &pool,
            uid,
            "never",
            "immediately",
            Some("myhandle"),
            None,
            None,
        )
        .await;
        set_chat_id(&pool, uid, "12345").await;
        let showing_id = make_showing(&pool, "F1").await;
        append_showing_for_users(
            &pool,
            showing_id,
            &[get_preferences(&pool, uid).await.unwrap()],
        )
        .await
        .unwrap();
        let batch_id = open_batch_id(&pool, uid, "telegram").await;

        let tg = FlakyTelegram {
            attempts: Arc::new(Mutex::new(0)),
            sent: Arc::new(Mutex::new(Vec::new())),
        };
        let sent = process_due_batches(&ctx(&pool, None, Some(&tg)), at(18, 12))
            .await
            .unwrap();
        assert_eq!(sent, 0);
        assert_eq!(batch_status(&pool, batch_id).await, "failed");
        let (error_count,): (i32,) =
            sqlx::query_as("SELECT error_count FROM notification_batch WHERE id = $1")
                .bind(batch_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(error_count, 1);

        sqlx::query("UPDATE notification_batch SET updated_at = $2 WHERE id = $1")
            .bind(batch_id)
            .bind(at(18, 10))
            .execute(&pool)
            .await
            .unwrap();
        let sent = process_due_batches(&ctx(&pool, None, Some(&tg)), at(19, 0))
            .await
            .unwrap();
        assert_eq!(sent, 1);
        assert_eq!(batch_status(&pool, batch_id).await, "sent");
        assert_eq!(tg.sent.lock().unwrap().len(), 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn gc_deletes_failed_batch_older_than_max_retry_age(pool: PgPool) {
        let uid = make_user(&pool, "k@x.com").await;
        let old_failed = create_failed_batch(&pool, uid, "email", at(10, 0)).await;
        let recent_failed = create_failed_batch(&pool, uid, "telegram", at(18, 10)).await;

        let deleted = crate::notification::db::gc_failed_batches(&pool, 168, at(18, 12))
            .await
            .unwrap();
        assert_eq!(deleted, 1);

        let (old_count,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM notification_batch WHERE id = $1")
                .bind(old_failed)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(old_count, 0);
        let (recent_count,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM notification_batch WHERE id = $1")
                .bind(recent_failed)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(recent_count, 1);
    }

    async fn create_failed_batch(
        pool: &PgPool,
        uid: i64,
        layer: &str,
        updated_at: DateTime<Utc>,
    ) -> i64 {
        let batch_id = crate::notification::db::create_empty_batch(pool, uid, layer)
            .await
            .unwrap();
        crate::notification::db::mark_batch_failed(pool, batch_id, "boom")
            .await
            .unwrap();
        sqlx::query("UPDATE notification_batch SET updated_at = $2 WHERE id = $1")
            .bind(batch_id)
            .bind(updated_at)
            .execute(pool)
            .await
            .unwrap();
        batch_id
    }
}
