use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::notification::rules::{MatchableShowing, Rule};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct NotificationPreferences {
    pub user_id: i64,
    pub email_enabled: bool,
    pub telegram_enabled: bool,
    pub telegram_handle: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub digest_anchor: DateTime<Utc>,
    pub digest_hour: i32,
    #[allow(dead_code)]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub struct PreferenceUpdate {
    pub email_enabled: Option<bool>,
    pub telegram_enabled: Option<bool>,
    pub telegram_handle: Option<String>,
    pub digest_anchor: Option<DateTime<Utc>>,
    pub digest_hour: Option<i32>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct DueBatch {
    pub batch_id: i64,
    pub user_id: i64,
    pub layer: String,
    pub frequency: String,
    pub digest_anchor: DateTime<Utc>,
    pub digest_hour: i32,
    pub created_at: DateTime<Utc>,
    // mapped DB column, currently read only by tests
    #[allow(dead_code)]
    pub error_count: i32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct NotificationRule {
    pub id: i64,
    #[allow(dead_code)]
    pub user_id: i64,
    pub position: i32,
    pub cinema_id: Option<i64>,
    pub features: Vec<String>,
    pub title_substring: Option<String>,
    pub frequency: String,
    pub channels: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RuleInput {
    pub cinema_id: Option<i64>,
    pub features: Vec<String>,
    pub title_substring: Option<String>,
    pub frequency: String,
    pub channels: Vec<String>,
}

pub async fn list_rules(pool: &PgPool, user_id: i64) -> sqlx::Result<Vec<NotificationRule>> {
    sqlx::query_as(
        "SELECT id, user_id, position, cinema_id, features, title_substring, frequency, channels
         FROM notification_rule WHERE user_id = $1 ORDER BY position",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn replace_rules(
    pool: &PgPool,
    user_id: i64,
    input: &[RuleInput],
) -> sqlx::Result<Vec<NotificationRule>> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM notification_rule WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    for (i, r) in input.iter().enumerate() {
        sqlx::query(
            "INSERT INTO notification_rule
               (user_id, position, cinema_id, features, title_substring, frequency, channels)
             VALUES ($1, $2, $3, $4, NULLIF($5, ''), $6, $7)",
        )
        .bind(user_id)
        .bind(i as i32)
        .bind(r.cinema_id)
        .bind(&r.features)
        .bind(r.title_substring.as_deref())
        .bind(&r.frequency)
        .bind(&r.channels)
        .execute(&mut *tx)
        .await?;
    }
    let rows = sqlx::query_as::<_, NotificationRule>(
        "SELECT id, user_id, position, cinema_id, features, title_substring, frequency, channels
         FROM notification_rule WHERE user_id = $1 ORDER BY position",
    )
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows)
}

#[derive(Debug, Clone)]
pub struct UserRules {
    pub user_id: i64,
    pub email_enabled: bool,
    pub telegram_enabled: bool,
    pub telegram_chat_id: Option<String>,
    #[allow(dead_code)]
    pub digest_anchor: DateTime<Utc>,
    #[allow(dead_code)]
    pub digest_hour: i32,
    pub rules: Vec<Rule>,
}

impl From<NotificationPreferences> for UserRules {
    fn from(p: NotificationPreferences) -> Self {
        UserRules {
            user_id: p.user_id,
            email_enabled: p.email_enabled,
            telegram_enabled: p.telegram_enabled,
            telegram_chat_id: p.telegram_chat_id,
            digest_anchor: p.digest_anchor,
            digest_hour: p.digest_hour,
            rules: Vec::new(),
        }
    }
}

pub async fn list_active_users_with_rules(pool: &PgPool) -> sqlx::Result<Vec<UserRules>> {
    let prefs: Vec<NotificationPreferences> = sqlx::query_as(
        "SELECT user_id, email_enabled, telegram_enabled, telegram_handle,
                telegram_chat_id, digest_anchor, digest_hour, updated_at
         FROM notification_preferences
         WHERE email_enabled
            OR (telegram_enabled AND telegram_chat_id IS NOT NULL)
         ORDER BY user_id",
    )
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(prefs.len());
    for p in prefs {
        let rules: Vec<NotificationRule> = sqlx::query_as(
            "SELECT id, user_id, position, cinema_id, features, title_substring, frequency, channels
             FROM notification_rule WHERE user_id = $1 ORDER BY position",
        )
        .bind(p.user_id)
        .fetch_all(pool)
        .await?;
        out.push(UserRules {
            user_id: p.user_id,
            email_enabled: p.email_enabled,
            telegram_enabled: p.telegram_enabled,
            telegram_chat_id: p.telegram_chat_id,
            digest_anchor: p.digest_anchor,
            digest_hour: p.digest_hour,
            rules: rules
                .into_iter()
                .map(|r| Rule {
                    cinema_id: r.cinema_id,
                    features: r.features,
                    title_substring: r.title_substring,
                    frequency: r.frequency,
                    channels: r.channels,
                })
                .collect(),
        });
    }
    Ok(out)
}

pub async fn load_matchable_showings(
    pool: &PgPool,
    showing_ids: &[i64],
) -> sqlx::Result<Vec<MatchableShowing>> {
    let rows: Vec<(i64, i64, Vec<String>, String)> = sqlx::query_as(
        "SELECT s.id, m.cinema_id, s.features, m.title
         FROM showing s JOIN movie m ON m.id = s.movie_id
         WHERE s.id = ANY($1)",
    )
    .bind(showing_ids)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(showing_id, cinema_id, features, title)| MatchableShowing {
                showing_id,
                cinema_id,
                features,
                title,
            },
        )
        .collect())
}

pub async fn list_cinemas(pool: &PgPool) -> sqlx::Result<Vec<(i64, String)>> {
    sqlx::query_as("SELECT id, name FROM cinema ORDER BY id")
        .fetch_all(pool)
        .await
}

pub async fn get_preferences(pool: &PgPool, user_id: i64) -> sqlx::Result<NotificationPreferences> {
    let created_at: Option<(DateTime<Utc>,)> =
        sqlx::query_as("SELECT created_at FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    let Some((created_at,)) = created_at else {
        return Err(sqlx::Error::RowNotFound);
    };
    let row = sqlx::query_as::<_, NotificationPreferences>(
        "SELECT user_id, email_enabled, telegram_enabled, telegram_handle,
                telegram_chat_id, digest_anchor, digest_hour, updated_at
         FROM notification_preferences WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(match row {
        Some(prefs) => prefs,
        None => NotificationPreferences {
            user_id,
            email_enabled: false,
            telegram_enabled: false,
            telegram_handle: None,
            telegram_chat_id: None,
            digest_anchor: created_at,
            digest_hour: 9,
            updated_at: created_at,
        },
    })
}

pub async fn upsert_preferences(
    pool: &PgPool,
    user_id: i64,
    dto: PreferenceUpdate,
) -> sqlx::Result<NotificationPreferences> {
    let existing = get_preferences(pool, user_id).await?;
    let new_handle = dto
        .telegram_handle
        .map(|raw| raw.trim().trim_start_matches('@').to_lowercase())
        .filter(|h| !h.is_empty());
    let clear_chat =
        new_handle.is_none() || new_handle.as_deref() != existing.telegram_handle.as_deref();
    let chat_id = if clear_chat {
        None
    } else {
        existing.telegram_chat_id.clone()
    };
    let email_enabled = dto.email_enabled.unwrap_or(existing.email_enabled);
    let telegram_enabled = dto.telegram_enabled.unwrap_or(existing.telegram_enabled);
    let digest_anchor = dto.digest_anchor.unwrap_or(existing.digest_anchor);
    let digest_hour = dto.digest_hour.unwrap_or(existing.digest_hour);
    sqlx::query_as::<_, NotificationPreferences>(
        "INSERT INTO notification_preferences
           (user_id, email_enabled, telegram_enabled, telegram_handle,
            telegram_chat_id, digest_anchor, digest_hour, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, now())
         ON CONFLICT (user_id) DO UPDATE SET
           email_enabled    = EXCLUDED.email_enabled,
           telegram_enabled = EXCLUDED.telegram_enabled,
           telegram_handle  = EXCLUDED.telegram_handle,
           telegram_chat_id  = EXCLUDED.telegram_chat_id,
           digest_anchor    = EXCLUDED.digest_anchor,
           digest_hour      = EXCLUDED.digest_hour,
           updated_at       = now()
         RETURNING user_id, email_enabled, telegram_enabled, telegram_handle,
                   telegram_chat_id, digest_anchor, digest_hour, updated_at",
    )
    .bind(user_id)
    .bind(email_enabled)
    .bind(telegram_enabled)
    .bind(new_handle)
    .bind(chat_id)
    .bind(digest_anchor)
    .bind(digest_hour)
    .fetch_one(pool)
    .await
}

pub async fn get_or_create_open_batch(
    pool: &PgPool,
    user_id: i64,
    layer: &str,
    frequency: &str,
) -> sqlx::Result<i64> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO notification_batch (user_id, layer, frequency) VALUES ($1, $2, $3)
         ON CONFLICT (user_id, layer, frequency) WHERE status = 'pending'
         DO UPDATE SET updated_at = now()
         RETURNING id",
    )
    .bind(user_id)
    .bind(layer)
    .bind(frequency)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub async fn append_showing_to_batch(
    pool: &PgPool,
    batch_id: i64,
    showing_id: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO notification_batch_showing (batch_id, showing_id) VALUES ($1, $2)
         ON CONFLICT DO NOTHING",
    )
    .bind(batch_id)
    .bind(showing_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_due_batches(pool: &PgPool, now: DateTime<Utc>) -> sqlx::Result<Vec<DueBatch>> {
    sqlx::query_as(
        "SELECT b.id AS batch_id, b.user_id, b.layer, b.frequency AS frequency,
                COALESCE(p.digest_anchor, u.created_at) AS digest_anchor,
                COALESCE(p.digest_hour, 9) AS digest_hour,
                b.created_at, b.error_count
         FROM notification_batch b
         JOIN users u ON u.id = b.user_id
         LEFT JOIN notification_preferences p ON p.user_id = b.user_id
         WHERE b.status = 'pending'
            OR (b.status = 'failed'
                AND b.updated_at + make_interval(hours => LEAST(POWER(2, LEAST(b.error_count, 5))::int, 24)) <= $1)
         ORDER BY b.id",
    )
    .bind(now)
    .fetch_all(pool)
    .await
}

pub async fn mark_batch_sending(pool: &PgPool, batch_id: i64) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE notification_batch SET status = 'sending', updated_at = now() WHERE id = $1",
    )
    .bind(batch_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_batch_sent(pool: &PgPool, batch_id: i64) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE notification_batch SET status = 'sent', updated_at = now(), sent_at = now() WHERE id = $1",
    )
    .bind(batch_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_batch_failed(pool: &PgPool, batch_id: i64, error: &str) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE notification_batch
         SET status = 'failed', updated_at = now(), error_count = error_count + 1, last_error = $2
         WHERE id = $1",
    )
    .bind(batch_id)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn gc_failed_batches(
    pool: &PgPool,
    max_retry_age_hours: u64,
    now: DateTime<Utc>,
) -> sqlx::Result<u64> {
    Ok(sqlx::query(
        "DELETE FROM notification_batch
          WHERE status = 'failed'
            AND updated_at + make_interval(hours => $1::int) <= $2",
    )
    .bind(max_retry_age_hours as i64)
    .bind(now)
    .execute(pool)
    .await?
    .rows_affected())
}

pub async fn create_empty_batch(
    pool: &PgPool,
    user_id: i64,
    layer: &str,
    frequency: &str,
) -> sqlx::Result<i64> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO notification_batch (user_id, layer, frequency) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(user_id)
    .bind(layer)
    .bind(frequency)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub async fn delete_open_batch(pool: &PgPool, user_id: i64, layer: &str) -> sqlx::Result<()> {
    sqlx::query(
        "DELETE FROM notification_batch WHERE user_id = $1 AND layer = $2 AND status = 'pending'",
    )
    .bind(user_id)
    .bind(layer)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

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
            },
        )
        .await
        .unwrap();
    }

    async fn make_user(pool: &PgPool, email: &str) -> i64 {
        crate::db::find_or_create_user(pool, "email", email, email)
            .await
            .unwrap()
    }

    async fn make_showing(pool: &PgPool) -> i64 {
        let mid = crate::db::upsert_movie(pool, "Cineplexx Linz", "F1", None, &[], None, None)
            .await
            .unwrap();
        let start = Utc::now() + Duration::days(1);
        assert!(crate::db::insert_showing(
            pool,
            mid,
            start,
            "OV",
            "Saal 6",
            "https://x",
            Utc::now(),
            &[],
        )
        .await
        .unwrap()
        .is_some());
        sqlx::query_as::<_, (i64,)>("SELECT id FROM showing WHERE movie_id = $1")
            .bind(mid)
            .fetch_one(pool)
            .await
            .unwrap()
            .0
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn preferences_defaults_and_upsert(pool: PgPool) {
        let uid = make_user(&pool, "a@b.com").await;
        let prefs = get_preferences(&pool, uid).await.unwrap();
        assert!(!prefs.email_enabled);
        assert!(!prefs.telegram_enabled);
        assert!(prefs.telegram_handle.is_none());
        assert_eq!(prefs.digest_hour, 9);

        let updated = upsert_preferences(
            &pool,
            uid,
            PreferenceUpdate {
                email_enabled: Some(true),
                telegram_enabled: Some(false),
                telegram_handle: Some("@MyHandle".into()),
                digest_anchor: None,
                digest_hour: Some(10),
            },
        )
        .await
        .unwrap();
        assert!(updated.email_enabled);
        assert!(!updated.telegram_enabled);
        assert_eq!(updated.telegram_handle.as_deref(), Some("myhandle"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_preferences_defaults_derive_anchor_from_user(pool: PgPool) {
        let uid = make_user(&pool, "c@d.com").await;
        let created: (DateTime<Utc>,) =
            sqlx::query_as("SELECT created_at FROM users WHERE id = $1")
                .bind(uid)
                .fetch_one(&pool)
                .await
                .unwrap();
        let prefs = get_preferences(&pool, uid).await.unwrap();
        assert_eq!(prefs.digest_anchor, created.0);
        assert!(!prefs.email_enabled);
        assert_eq!(prefs.digest_hour, 9);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn upsert_clears_chat_id_on_handle_change_or_clear(pool: PgPool) {
        let uid = make_user(&pool, "h@x.com").await;
        upsert_preferences(
            &pool,
            uid,
            PreferenceUpdate {
                email_enabled: None,
                telegram_enabled: Some(true),
                telegram_handle: Some("myhandle".into()),
                digest_anchor: None,
                digest_hour: None,
            },
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE notification_preferences SET telegram_chat_id = '12345' WHERE user_id = $1",
        )
        .bind(uid)
        .execute(&pool)
        .await
        .unwrap();

        // re-saving the same handle keeps the verified chat id
        let kept = upsert_preferences(
            &pool,
            uid,
            PreferenceUpdate {
                telegram_handle: Some("  @MyHandle ".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(kept.telegram_handle.as_deref(), Some("myhandle"));
        assert_eq!(kept.telegram_chat_id.as_deref(), Some("12345"));

        // clearing the handle also clears the chat id
        let cleared = upsert_preferences(
            &pool,
            uid,
            PreferenceUpdate {
                telegram_handle: None,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(cleared.telegram_handle.is_none());
        assert!(cleared.telegram_chat_id.is_none());

        // changing the handle clears the old chat id (must re-verify)
        upsert_preferences(
            &pool,
            uid,
            PreferenceUpdate {
                telegram_handle: Some("newhandle".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE notification_preferences SET telegram_chat_id = '999' WHERE user_id = $1",
        )
        .bind(uid)
        .execute(&pool)
        .await
        .unwrap();
        let changed = upsert_preferences(
            &pool,
            uid,
            PreferenceUpdate {
                telegram_handle: Some("other".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(changed.telegram_handle.as_deref(), Some("other"));
        assert!(changed.telegram_chat_id.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_or_create_open_batch_is_idempotent(pool: PgPool) {
        let uid = make_user(&pool, "b@x.com").await;
        let id1 = get_or_create_open_batch(&pool, uid, "email", "immediately")
            .await
            .unwrap();
        let id2 = get_or_create_open_batch(&pool, uid, "email", "immediately")
            .await
            .unwrap();
        assert_eq!(id1, id2);
        let id3 = get_or_create_open_batch(&pool, uid, "telegram", "immediately")
            .await
            .unwrap();
        assert_ne!(id1, id3);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn append_showing_dedups(pool: PgPool) {
        let uid = make_user(&pool, "d@x.com").await;
        let batch_id = get_or_create_open_batch(&pool, uid, "email", "immediately")
            .await
            .unwrap();
        let showing_id = make_showing(&pool).await;
        append_showing_to_batch(&pool, batch_id, showing_id)
            .await
            .unwrap();
        append_showing_to_batch(&pool, batch_id, showing_id)
            .await
            .unwrap();
        let count: (i64,) =
            sqlx::query_as("SELECT count(*) FROM notification_batch_showing WHERE batch_id = $1")
                .bind(batch_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn mark_sending_and_sent(pool: PgPool) {
        let uid = make_user(&pool, "e@x.com").await;
        let batch_id = create_empty_batch(&pool, uid, "email", "immediately")
            .await
            .unwrap();
        mark_batch_sending(&pool, batch_id).await.unwrap();
        let status: (String,) =
            sqlx::query_as("SELECT status FROM notification_batch WHERE id = $1")
                .bind(batch_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status.0, "sending");
        mark_batch_sent(&pool, batch_id).await.unwrap();
        let (status, sent_at): (String, Option<DateTime<Utc>>) =
            sqlx::query_as("SELECT status, sent_at FROM notification_batch WHERE id = $1")
                .bind(batch_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "sent");
        assert!(sent_at.is_some());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn mark_failed_increments_error_count(pool: PgPool) {
        let uid = make_user(&pool, "f@x.com").await;
        let batch_id = create_empty_batch(&pool, uid, "telegram", "immediately")
            .await
            .unwrap();
        mark_batch_failed(&pool, batch_id, "boom").await.unwrap();
        mark_batch_failed(&pool, batch_id, "boom again")
            .await
            .unwrap();
        let (status, error_count, last_error): (String, i32, Option<String>) = sqlx::query_as(
            "SELECT status, error_count, last_error FROM notification_batch WHERE id = $1",
        )
        .bind(batch_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(status, "failed");
        assert_eq!(error_count, 2);
        assert_eq!(last_error.as_deref(), Some("boom again"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_empty_and_delete_open_batch(pool: PgPool) {
        let uid = make_user(&pool, "g@x.com").await;
        let batch_id = create_empty_batch(&pool, uid, "email", "immediately")
            .await
            .unwrap();
        let showing_id = make_showing(&pool).await;
        append_showing_to_batch(&pool, batch_id, showing_id)
            .await
            .unwrap();
        delete_open_batch(&pool, uid, "email").await.unwrap();
        let rows: (i64,) = sqlx::query_as("SELECT count(*) FROM notification_batch WHERE id = $1")
            .bind(batch_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rows.0, 0);
        let links: (i64,) =
            sqlx::query_as("SELECT count(*) FROM notification_batch_showing WHERE batch_id = $1")
                .bind(batch_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(links.0, 0);

        let sent_id = create_empty_batch(&pool, uid, "telegram", "immediately")
            .await
            .unwrap();
        mark_batch_sent(&pool, sent_id).await.unwrap();
        delete_open_batch(&pool, uid, "telegram").await.unwrap();
        let exists: (i64,) =
            sqlx::query_as("SELECT count(*) FROM notification_batch WHERE id = $1")
                .bind(sent_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(exists.0, 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn open_batches_are_per_frequency(pool: PgPool) {
        let uid = make_user(&pool, "freq@x.com").await;
        let a = get_or_create_open_batch(&pool, uid, "email", "immediately")
            .await
            .unwrap();
        let b = get_or_create_open_batch(&pool, uid, "email", "3")
            .await
            .unwrap();
        let a2 = get_or_create_open_batch(&pool, uid, "email", "immediately")
            .await
            .unwrap();
        assert_ne!(a, b);
        assert_eq!(a, a2, "idempotent per (user, layer, frequency)");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_due_batches_reads_batch_frequency(pool: PgPool) {
        let uid = make_user(&pool, "due@x.com").await;
        let anchor = Utc::now() - Duration::days(10);
        prefs_for(&pool, uid, true, false, None, Some(anchor), Some(9)).await;
        let imm = get_or_create_open_batch(&pool, uid, "email", "immediately")
            .await
            .unwrap();
        let three = get_or_create_open_batch(&pool, uid, "email", "3")
            .await
            .unwrap();
        let due = get_due_batches(&pool, Utc::now()).await.unwrap();
        let ids: Vec<i64> = due.iter().map(|d| d.batch_id).collect();
        assert!(ids.contains(&imm), "immediately batch should be returned");
        assert!(
            ids.contains(&three),
            "3-day batch should also be returned (filtered by batch_is_due, not here)"
        );
        let imm_row = due.iter().find(|d| d.batch_id == imm).unwrap();
        assert_eq!(imm_row.frequency, "immediately");
        let three_row = due.iter().find(|d| d.batch_id == three).unwrap();
        assert_eq!(three_row.frequency, "3");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_due_batches_returns_pending_and_retryable_failed(pool: PgPool) {
        let email_user = make_user(&pool, "i1@x.com").await;
        let telegram_user = make_user(&pool, "i2@x.com").await;
        let sent_user = make_user(&pool, "i3@x.com").await;

        let pending_email = get_or_create_open_batch(&pool, email_user, "email", "immediately")
            .await
            .unwrap();
        let pending_tg = get_or_create_open_batch(&pool, telegram_user, "telegram", "3")
            .await
            .unwrap();

        let recent_failed = create_empty_batch(&pool, email_user, "telegram", "immediately")
            .await
            .unwrap();
        mark_batch_failed(&pool, recent_failed, "x").await.unwrap();

        let retry_failed = create_empty_batch(&pool, telegram_user, "email", "3")
            .await
            .unwrap();
        mark_batch_failed(&pool, retry_failed, "y").await.unwrap();
        sqlx::query(
            "UPDATE notification_batch SET updated_at = now() - interval '3 hours' WHERE id = $1",
        )
        .bind(retry_failed)
        .execute(&pool)
        .await
        .unwrap();

        let sent = create_empty_batch(&pool, sent_user, "telegram", "immediately")
            .await
            .unwrap();
        mark_batch_sent(&pool, sent).await.unwrap();

        let due = get_due_batches(&pool, Utc::now()).await.unwrap();
        let ids: Vec<i64> = due.iter().map(|d| d.batch_id).collect();
        assert!(ids.contains(&pending_email));
        assert!(ids.contains(&pending_tg));
        assert!(ids.contains(&retry_failed));
        assert!(!ids.contains(&recent_failed));
        assert!(!ids.contains(&sent));

        assert_eq!(
            due.iter()
                .find(|d| d.batch_id == pending_email)
                .unwrap()
                .frequency,
            "immediately"
        );
        assert_eq!(
            due.iter()
                .find(|d| d.batch_id == pending_tg)
                .unwrap()
                .frequency,
            "3"
        );
        assert_eq!(
            due.iter()
                .find(|d| d.batch_id == retry_failed)
                .unwrap()
                .frequency,
            "3"
        );
    }

    async fn make_rule_user(pool: &PgPool) -> i64 {
        crate::db::find_or_create_user(pool, "email", "rules@x.com", "rules@x.com")
            .await
            .unwrap()
    }

    fn input(
        cinema_id: Option<i64>,
        features: &[&str],
        title: Option<&str>,
        freq: &str,
    ) -> RuleInput {
        RuleInput {
            cinema_id,
            features: features.iter().map(|s| s.to_string()).collect(),
            title_substring: title.map(|s| s.to_string()),
            frequency: freq.to_string(),
            channels: vec!["email".into()],
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn replace_rules_inserts_in_order(pool: PgPool) {
        let uid = make_rule_user(&pool).await;
        let inserted = replace_rules(
            &pool,
            uid,
            &[
                input(Some(1), &["IMAX", "Atmos"], None, "immediately"),
                input(None, &[], None, "3"),
            ],
        )
        .await
        .unwrap();
        assert_eq!(inserted.len(), 2);
        assert_eq!(inserted[0].position, 0);
        assert_eq!(inserted[0].features, vec!["IMAX", "Atmos"]);
        assert_eq!(inserted[1].position, 1);
        assert_eq!(inserted[1].frequency, "3");
        let listed = list_rules(&pool, uid).await.unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, inserted[0].id);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn replace_rules_is_replacement(pool: PgPool) {
        let uid = make_rule_user(&pool).await;
        replace_rules(&pool, uid, &[input(None, &[], None, "3")])
            .await
            .unwrap();
        let replaced = replace_rules(
            &pool,
            uid,
            &[input(Some(1), &["IMAX"], None, "immediately")],
        )
        .await
        .unwrap();
        assert_eq!(replaced.len(), 1);
        assert_eq!(replaced[0].frequency, "immediately");
        assert_eq!(list_rules(&pool, uid).await.unwrap().len(), 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn replace_rules_round_trips_channels(pool: PgPool) {
        let uid = make_rule_user(&pool).await;
        let inserted = replace_rules(
            &pool,
            uid,
            &[
                RuleInput {
                    cinema_id: None,
                    features: vec![],
                    title_substring: None,
                    frequency: "3".into(),
                    channels: vec!["email".into(), "telegram".into()],
                },
                RuleInput {
                    cinema_id: None,
                    features: vec![],
                    title_substring: None,
                    frequency: "immediately".into(),
                    channels: vec!["telegram".into()],
                },
            ],
        )
        .await
        .unwrap();
        assert_eq!(inserted[0].channels, vec!["email", "telegram"]);
        assert_eq!(inserted[1].channels, vec!["telegram"]);
        let listed = list_rules(&pool, uid).await.unwrap();
        assert_eq!(listed[0].channels, vec!["email", "telegram"]);
        assert_eq!(listed[1].channels, vec!["telegram"]);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn list_cinemas_returns_known(pool: PgPool) {
        let cinemas = list_cinemas(&pool).await.unwrap();
        let names: Vec<String> = cinemas.iter().map(|(_, n)| n.clone()).collect();
        assert!(names.contains(&"Cineplexx Linz".to_string()));
        assert!(names.contains(&"Megaplex PlusCity".to_string()));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn list_active_users_with_rules_filters_inactive(pool: PgPool) {
        let active = make_user(&pool, "act@x.com").await;
        let inactive = make_user(&pool, "inact@x.com").await;
        let tg_unverified = make_user(&pool, "tg@x.com").await;
        prefs_for(&pool, active, true, false, None, None, None).await;
        prefs_for(&pool, inactive, false, false, None, None, None).await;
        prefs_for(&pool, tg_unverified, false, true, Some("h"), None, None).await;
        replace_rules(
            &pool,
            active,
            &[RuleInput {
                cinema_id: None,
                features: vec![],
                title_substring: None,
                frequency: "3".into(),
                channels: vec!["email".into()],
            }],
        )
        .await
        .unwrap();

        let users = list_active_users_with_rules(&pool).await.unwrap();
        let ids: Vec<i64> = users.iter().map(|u| u.user_id).collect();
        assert!(ids.contains(&active));
        assert!(!ids.contains(&inactive));
        assert!(
            !ids.contains(&tg_unverified),
            "telegram_enabled but unverified must not be active"
        );
        let a = users.iter().find(|u| u.user_id == active).unwrap();
        assert_eq!(a.rules.len(), 1);
        assert_eq!(a.rules[0].frequency, "3");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn load_matchable_showings_joins_cinema_features(pool: PgPool) {
        let mid = crate::db::upsert_movie(&pool, "Cineplexx Linz", "F1", None, &[], None, None)
            .await
            .unwrap();
        let sid = crate::db::insert_showing(
            &pool,
            mid,
            Utc::now() + Duration::days(1),
            "OV",
            "Saal 6",
            "https://x",
            Utc::now(),
            &["OV".into(), "2D".into()],
        )
        .await
        .unwrap()
        .unwrap();
        let ms = load_matchable_showings(&pool, &[sid]).await.unwrap();
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].showing_id, sid);
        assert_eq!(ms[0].title, "F1");
        assert!(ms[0].features.contains(&"OV".to_string()));
    }
}
