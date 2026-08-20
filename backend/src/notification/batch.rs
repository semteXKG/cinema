use crate::models::{MovieMeta, Showing};
use crate::notification::db::{self, DueBatch, UserRules};
use crate::notification::rules::{first_match, MatchableShowing};
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

pub async fn route_showing_for_users(
    pool: &PgPool,
    showing_id: i64,
    showing: &MatchableShowing,
    users: &[UserRules],
) -> sqlx::Result<Vec<(i64, String)>> {
    let mut affected: Vec<(i64, String)> = Vec::new();
    for u in users {
        let freq = first_match(&u.rules, showing).unwrap_or("never");
        if freq == "never" {
            continue;
        }
        if u.email_enabled {
            let batch_id = db::get_or_create_open_batch(pool, u.user_id, "email", freq).await?;
            db::append_showing_to_batch(pool, batch_id, showing_id).await?;
            affected.push((u.user_id, "email".to_string()));
        }
        if u.telegram_enabled && u.telegram_chat_id.is_some() {
            let batch_id = db::get_or_create_open_batch(pool, u.user_id, "telegram", freq).await?;
            db::append_showing_to_batch(pool, batch_id, showing_id).await?;
            affected.push((u.user_id, "telegram".to_string()));
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
    if let Err(e) =
        db::create_empty_batch(ctx.pool, batch.user_id, &batch.layer, &batch.frequency).await
    {
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
        "SELECT c.name AS cinema, m.title AS movie, s.start, s.version, s.hall, s.url,
                m.runtime_min, m.genres
         FROM notification_batch_showing nbs
         JOIN showing s ON s.id = nbs.showing_id
         JOIN movie m ON m.id = s.movie_id
         JOIN cinema c ON c.id = m.cinema_id
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
            features: vec![],
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
    use crate::notification::db::{upsert_preferences, PreferenceUpdate};
    use crate::notification::rules::MatchableShowing;
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
            &[],
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
        email_enabled: bool,
        telegram_enabled: bool,
        handle: Option<&str>,
        digest_anchor: Option<DateTime<Utc>>,
        digest_hour: Option<i32>,
    ) {
        upsert_preferences(
            pool,
            uid,
            PreferenceUpdate {
                email_enabled: Some(email_enabled),
                telegram_enabled: Some(telegram_enabled),
                telegram_handle: handle.map(|s| s.to_string()),
                digest_anchor,
                digest_hour,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }

    fn matchable(
        showing_id: i64,
        cinema_id: i64,
        features: &[&str],
        title: &str,
    ) -> MatchableShowing {
        MatchableShowing {
            showing_id,
            cinema_id,
            features: features.iter().map(|s| s.to_string()).collect(),
            title: title.to_string(),
        }
    }

    fn user_rules(
        uid: i64,
        email: bool,
        tg: bool,
        chat: Option<&str>,
        rules: Vec<crate::notification::rules::Rule>,
    ) -> crate::notification::db::UserRules {
        crate::notification::db::UserRules {
            user_id: uid,
            email_enabled: email,
            telegram_enabled: tg,
            telegram_chat_id: chat.map(|s| s.to_string()),
            digest_anchor: at(16, 9),
            digest_hour: 9,
            rules,
        }
    }

    fn rule(freq: &str) -> crate::notification::rules::Rule {
        crate::notification::rules::Rule {
            cinema_id: None,
            features: vec![],
            title_substring: None,
            frequency: freq.to_string(),
        }
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

    async fn open_batch_id(pool: &PgPool, uid: i64, layer: &str, frequency: &str) -> i64 {
        sqlx::query_as::<_, (i64,)>(
            "SELECT id FROM notification_batch WHERE user_id=$1 AND layer=$2 AND frequency=$3 AND status='pending'",
        ).bind(uid).bind(layer).bind(frequency).fetch_one(pool).await.unwrap().0
    }

    async fn batch_status(pool: &PgPool, batch_id: i64) -> String {
        sqlx::query_as::<_, (String,)>("SELECT status FROM notification_batch WHERE id = $1")
            .bind(batch_id)
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
    async fn immediate_rule_routes_to_immediate_batch(pool: PgPool) {
        let uid = make_user(&pool, "a@b.com").await;
        prefs_for(&pool, uid, true, false, None, None, None).await;
        let sid = make_showing(&pool, "The Odyssey").await;
        let m = matchable(sid, 1, &["OV"], "The Odyssey");
        let users = vec![user_rules(
            uid,
            true,
            false,
            None,
            vec![rule("immediately")],
        )];
        let affected = route_showing_for_users(&pool, sid, &m, &users)
            .await
            .unwrap();
        assert_eq!(affected, vec![(uid, "email".to_string())]);
        let n: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM notification_batch WHERE user_id=$1 AND layer='email' AND frequency='immediately' AND status='pending'",
        ).bind(uid).fetch_one(&pool).await.unwrap();
        assert_eq!(n.0, 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn never_match_creates_no_batch(pool: PgPool) {
        let uid = make_user(&pool, "c@d.com").await;
        prefs_for(&pool, uid, true, false, None, None, None).await;
        let sid = make_showing(&pool, "F1").await;
        let m = matchable(sid, 1, &[], "F1");
        let users = vec![user_rules(
            uid,
            true,
            false,
            None,
            vec![crate::notification::rules::Rule {
                cinema_id: Some(999),
                features: vec![],
                title_substring: None,
                frequency: "immediately".to_string(),
            }],
        )];
        let affected = route_showing_for_users(&pool, sid, &m, &users)
            .await
            .unwrap();
        assert!(affected.is_empty());
        let n: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM notification_batch WHERE user_id=$1 AND status='pending'",
        )
        .bind(uid)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(n.0, 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn disabled_layer_is_not_routed(pool: PgPool) {
        let uid = make_user(&pool, "e@f.com").await;
        prefs_for(&pool, uid, false, false, None, None, None).await;
        let sid = make_showing(&pool, "F1").await;
        let m = matchable(sid, 1, &["OV"], "F1");
        let users = vec![user_rules(
            uid,
            false,
            false,
            None,
            vec![rule("immediately")],
        )];
        let affected = route_showing_for_users(&pool, sid, &m, &users)
            .await
            .unwrap();
        assert!(affected.is_empty());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn immediate_batch_flushes_same_run(pool: PgPool) {
        let uid = make_user(&pool, "g@h.com").await;
        prefs_for(&pool, uid, true, false, None, None, None).await;
        let sid = make_showing(&pool, "F1").await;
        let m = matchable(sid, 1, &["OV"], "F1");
        let users = vec![user_rules(
            uid,
            true,
            false,
            None,
            vec![rule("immediately")],
        )];
        route_showing_for_users(&pool, sid, &m, &users)
            .await
            .unwrap();
        let email = RecordingEmail::default();
        let sent = process_due_batches(&ctx(&pool, Some(&email), None), at(18, 12))
            .await
            .unwrap();
        assert_eq!(sent, 1);
        assert!(email.sent.lock().unwrap()[0].2.contains("F1"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn three_day_batch_not_due_yet(pool: PgPool) {
        let uid = make_user(&pool, "i@j.com").await;
        prefs_for(&pool, uid, true, false, None, Some(at(16, 9)), Some(9)).await;
        let sid = make_showing(&pool, "F1").await;
        let m = matchable(sid, 1, &["OV"], "F1");
        let users = vec![user_rules(uid, true, false, None, vec![rule("3")])];
        route_showing_for_users(&pool, sid, &m, &users)
            .await
            .unwrap();
        let batch_id = open_batch_id(&pool, uid, "email", "3").await;
        set_batch_created_at(&pool, batch_id, at(19, 8)).await;
        let email = RecordingEmail::default();
        let sent = process_due_batches(&ctx(&pool, Some(&email), None), at(19, 8))
            .await
            .unwrap();
        assert_eq!(sent, 0);
        assert_eq!(batch_status(&pool, batch_id).await, "pending");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn failed_batch_is_retried(pool: PgPool) {
        let uid = make_user(&pool, "i@j.com").await;
        prefs_for(&pool, uid, false, true, Some("myhandle"), None, None).await;
        set_chat_id(&pool, uid, "12345").await;
        let sid = make_showing(&pool, "F1").await;
        let m = matchable(sid, 1, &["OV"], "F1");
        let users = vec![user_rules(
            uid,
            false,
            true,
            Some("12345"),
            vec![rule("immediately")],
        )];
        route_showing_for_users(&pool, sid, &m, &users)
            .await
            .unwrap();
        let batch_id = open_batch_id(&pool, uid, "telegram", "immediately").await;

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
        let old_failed = create_failed_batch(&pool, uid, "email", "immediately", at(10, 0)).await;
        let recent_failed =
            create_failed_batch(&pool, uid, "telegram", "immediately", at(18, 10)).await;

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
        frequency: &str,
        updated_at: DateTime<Utc>,
    ) -> i64 {
        let batch_id = crate::notification::db::create_empty_batch(pool, uid, layer, frequency)
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
