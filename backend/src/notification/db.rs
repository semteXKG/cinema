use chrono::{DateTime, Utc};
use sqlx::PgPool;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct NotificationPreferences {
    pub user_id: i64,
    pub email_frequency: String,
    pub telegram_frequency: String,
    pub telegram_handle: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub digest_anchor: DateTime<Utc>,
    pub digest_hour: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub struct PreferenceUpdate {
    pub email_frequency: Option<String>,
    pub telegram_frequency: Option<String>,
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
    pub error_count: i32,
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
        "SELECT user_id, email_frequency, telegram_frequency, telegram_handle,
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
            email_frequency: "never".into(),
            telegram_frequency: "never".into(),
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
    let email_frequency = dto.email_frequency.unwrap_or(existing.email_frequency);
    let telegram_frequency = dto
        .telegram_frequency
        .unwrap_or(existing.telegram_frequency);
    let digest_anchor = dto.digest_anchor.unwrap_or(existing.digest_anchor);
    let digest_hour = dto.digest_hour.unwrap_or(existing.digest_hour);
    sqlx::query_as::<_, NotificationPreferences>(
        "INSERT INTO notification_preferences
           (user_id, email_frequency, telegram_frequency, telegram_handle,
            telegram_chat_id, digest_anchor, digest_hour, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, now())
         ON CONFLICT (user_id) DO UPDATE SET
           email_frequency    = EXCLUDED.email_frequency,
           telegram_frequency = EXCLUDED.telegram_frequency,
           telegram_handle    = EXCLUDED.telegram_handle,
           telegram_chat_id   = EXCLUDED.telegram_chat_id,
           digest_anchor      = EXCLUDED.digest_anchor,
           digest_hour        = EXCLUDED.digest_hour,
           updated_at         = now()
         RETURNING user_id, email_frequency, telegram_frequency, telegram_handle,
                   telegram_chat_id, digest_anchor, digest_hour, updated_at",
    )
    .bind(user_id)
    .bind(email_frequency)
    .bind(telegram_frequency)
    .bind(new_handle)
    .bind(chat_id)
    .bind(digest_anchor)
    .bind(digest_hour)
    .fetch_one(pool)
    .await
}

pub async fn list_active_preferences(pool: &PgPool) -> sqlx::Result<Vec<NotificationPreferences>> {
    sqlx::query_as(
        "SELECT user_id, email_frequency, telegram_frequency, telegram_handle,
                telegram_chat_id, digest_anchor, digest_hour, updated_at
         FROM notification_preferences
         WHERE email_frequency != 'never'
            OR (telegram_frequency != 'never' AND telegram_chat_id IS NOT NULL)
         ORDER BY user_id",
    )
    .fetch_all(pool)
    .await
}

pub async fn get_or_create_open_batch(
    pool: &PgPool,
    user_id: i64,
    layer: &str,
) -> sqlx::Result<i64> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO notification_batch (user_id, layer) VALUES ($1, $2)
         ON CONFLICT (user_id, layer) WHERE status = 'pending'
         DO UPDATE SET updated_at = now()
         RETURNING id",
    )
    .bind(user_id)
    .bind(layer)
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
        "SELECT b.id AS batch_id, b.user_id, b.layer,
                COALESCE(CASE WHEN b.layer = 'email'
                              THEN p.email_frequency ELSE p.telegram_frequency END,
                         'never') AS frequency,
                COALESCE(p.digest_anchor, u.created_at) AS digest_anchor,
                COALESCE(p.digest_hour, 9) AS digest_hour,
                b.created_at,
                b.error_count
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

pub async fn create_empty_batch(pool: &PgPool, user_id: i64, layer: &str) -> sqlx::Result<i64> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO notification_batch (user_id, layer) VALUES ($1, $2) RETURNING id",
    )
    .bind(user_id)
    .bind(layer)
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
        crate::db::insert_showing(pool, mid, start, "OV", "Saal 6", "https://x", Utc::now())
            .await
            .unwrap();
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
        assert_eq!(prefs.email_frequency, "never");
        assert_eq!(prefs.digest_hour, 9);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn upsert_clears_chat_id_on_handle_change_or_clear(pool: PgPool) {
        let uid = make_user(&pool, "h@x.com").await;
        upsert_preferences(
            &pool,
            uid,
            PreferenceUpdate {
                email_frequency: None,
                telegram_frequency: Some("immediately".into()),
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
    async fn list_active_preferences_filters(pool: PgPool) {
        let email_active = make_user(&pool, "e1@x.com").await;
        let telegram_active = make_user(&pool, "e2@x.com").await;
        let telegram_unverified = make_user(&pool, "e3@x.com").await;
        let none_active = make_user(&pool, "e4@x.com").await;

        upsert_preferences(
            &pool,
            email_active,
            PreferenceUpdate {
                email_frequency: Some("immediately".into()),
                telegram_frequency: None,
                telegram_handle: None,
                digest_anchor: None,
                digest_hour: None,
            },
        )
        .await
        .unwrap();
        upsert_preferences(
            &pool,
            telegram_active,
            PreferenceUpdate {
                email_frequency: None,
                telegram_frequency: Some("3".into()),
                telegram_handle: Some("h2".into()),
                digest_anchor: None,
                digest_hour: None,
            },
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE notification_preferences SET telegram_chat_id = '111' WHERE user_id = $1",
        )
        .bind(telegram_active)
        .execute(&pool)
        .await
        .unwrap();
        upsert_preferences(
            &pool,
            telegram_unverified,
            PreferenceUpdate {
                email_frequency: None,
                telegram_frequency: Some("3".into()),
                telegram_handle: Some("h3".into()),
                digest_anchor: None,
                digest_hour: None,
            },
        )
        .await
        .unwrap();

        let active = list_active_preferences(&pool).await.unwrap();
        let ids: Vec<i64> = active.iter().map(|p| p.user_id).collect();
        assert!(ids.contains(&email_active));
        assert!(ids.contains(&telegram_active));
        assert!(!ids.contains(&telegram_unverified));
        assert!(!ids.contains(&none_active));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_or_create_open_batch_is_idempotent(pool: PgPool) {
        let uid = make_user(&pool, "b@x.com").await;
        let id1 = get_or_create_open_batch(&pool, uid, "email").await.unwrap();
        let id2 = get_or_create_open_batch(&pool, uid, "email").await.unwrap();
        assert_eq!(id1, id2);
        let id3 = get_or_create_open_batch(&pool, uid, "telegram")
            .await
            .unwrap();
        assert_ne!(id1, id3);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn append_showing_dedups(pool: PgPool) {
        let uid = make_user(&pool, "d@x.com").await;
        let batch_id = get_or_create_open_batch(&pool, uid, "email").await.unwrap();
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
        let batch_id = create_empty_batch(&pool, uid, "email").await.unwrap();
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
        let batch_id = create_empty_batch(&pool, uid, "telegram").await.unwrap();
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
        let batch_id = create_empty_batch(&pool, uid, "email").await.unwrap();
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

        let sent_id = create_empty_batch(&pool, uid, "telegram").await.unwrap();
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
    async fn get_due_batches_returns_pending_and_retryable_failed(pool: PgPool) {
        let email_user = make_user(&pool, "i1@x.com").await;
        let telegram_user = make_user(&pool, "i2@x.com").await;
        let sent_user = make_user(&pool, "i3@x.com").await;

        upsert_preferences(
            &pool,
            email_user,
            PreferenceUpdate {
                email_frequency: Some("immediately".into()),
                telegram_frequency: None,
                telegram_handle: None,
                digest_anchor: None,
                digest_hour: None,
            },
        )
        .await
        .unwrap();
        upsert_preferences(
            &pool,
            telegram_user,
            PreferenceUpdate {
                email_frequency: None,
                telegram_frequency: Some("3".into()),
                telegram_handle: None,
                digest_anchor: None,
                digest_hour: Some(7),
            },
        )
        .await
        .unwrap();

        let pending_email = get_or_create_open_batch(&pool, email_user, "email")
            .await
            .unwrap();
        let pending_tg = get_or_create_open_batch(&pool, telegram_user, "telegram")
            .await
            .unwrap();

        let recent_failed = create_empty_batch(&pool, email_user, "telegram")
            .await
            .unwrap();
        mark_batch_failed(&pool, recent_failed, "x").await.unwrap();

        let retry_failed = create_empty_batch(&pool, telegram_user, "email")
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

        let sent = create_empty_batch(&pool, sent_user, "telegram")
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

        let email_batch = due.iter().find(|d| d.batch_id == pending_email).unwrap();
        assert_eq!(email_batch.layer, "email");
        assert_eq!(email_batch.frequency, "immediately");
        let tg_batch = due.iter().find(|d| d.batch_id == pending_tg).unwrap();
        assert_eq!(tg_batch.layer, "telegram");
        assert_eq!(tg_batch.frequency, "3");
        assert_eq!(tg_batch.digest_hour, 7);
        let failed_batch = due.iter().find(|d| d.batch_id == retry_failed).unwrap();
        assert_eq!(failed_batch.frequency, "never");
        assert_eq!(failed_batch.error_count, 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_due_batches_high_error_count_retryable_without_overflow(pool: PgPool) {
        let uid = make_user(&pool, "j@x.com").await;
        let row: (i64,) = sqlx::query_as(
            "INSERT INTO notification_batch (user_id, layer, status, error_count, updated_at)
             VALUES ($1, 'email', 'failed', 40, now() - interval '25 hours')
             RETURNING id",
        )
        .bind(uid)
        .fetch_one(&pool)
        .await
        .unwrap();
        let batch_id = row.0;
        let due = get_due_batches(&pool, Utc::now()).await.unwrap();
        assert!(due.iter().any(|d| d.batch_id == batch_id));
    }
}
