use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;
use std::collections::HashSet;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ShowingView {
    pub cinema: String,
    pub movie: String,
    pub start: DateTime<Utc>,
    pub version: String,
    pub hall: String,
    pub url: String,
    pub runtime_min: Option<i32>,
    pub genres: Vec<String>,
    pub poster_file: Option<String>,
}

pub async fn upsert_movie(
    pool: &PgPool,
    cinema: &str,
    title: &str,
    runtime_min: Option<i32>,
    genres: &[String],
    poster_url: Option<&str>,
    poster_file: Option<&str>,
) -> sqlx::Result<i64> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO movie (cinema_id, title, runtime_min, genres, poster_url, poster_file)
         VALUES ((SELECT id FROM cinema WHERE name = $1), $2, $3, $4, $5, $6)
         ON CONFLICT (cinema_id, title) DO UPDATE SET
           runtime_min = EXCLUDED.runtime_min,
           genres      = EXCLUDED.genres,
           poster_url  = EXCLUDED.poster_url,
           poster_file = EXCLUDED.poster_file
         RETURNING id",
    )
    .bind(cinema)
    .bind(title)
    .bind(runtime_min)
    .bind(genres)
    .bind(poster_url)
    .bind(poster_file)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub async fn insert_showing(
    pool: &PgPool,
    movie_id: i64,
    start: DateTime<Utc>,
    version: &str,
    hall: &str,
    url: &str,
    first_seen: DateTime<Utc>,
) -> sqlx::Result<Option<i64>> {
    let row: Option<(i64,)> = sqlx::query_as(
        "INSERT INTO showing (movie_id, start, version, hall, url, first_seen_at)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (movie_id, start) DO NOTHING
         RETURNING id",
    )
    .bind(movie_id)
    .bind(start)
    .bind(version)
    .bind(hall)
    .bind(url)
    .bind(first_seen)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}

pub async fn upcoming_view(pool: &PgPool, since: DateTime<Utc>) -> sqlx::Result<Vec<ShowingView>> {
    sqlx::query_as(
        "SELECT c.name AS cinema, m.title AS movie, s.start, s.version, s.hall, s.url,
                m.runtime_min, m.genres, m.poster_file
         FROM showing s
         JOIN movie m ON m.id = s.movie_id
         JOIN cinema c ON c.id = m.cinema_id
         WHERE s.start >= $1
         ORDER BY s.start, c.name",
    )
    .bind(since)
    .fetch_all(pool)
    .await
}

pub async fn prune(pool: &PgPool, cutoff: DateTime<Utc>) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM showing WHERE start < $1")
        .bind(cutoff)
        .execute(pool)
        .await?;
    sqlx::query(
        "DELETE FROM movie m WHERE NOT EXISTS (SELECT 1 FROM showing s WHERE s.movie_id = m.id)",
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_source_status(
    pool: &PgPool,
    source: &str,
) -> sqlx::Result<Option<(String, Option<NaiveDate>)>> {
    sqlx::query_as("SELECT status, last_error_ping_date FROM source_status WHERE source = $1")
        .bind(source)
        .fetch_optional(pool)
        .await
}

pub async fn upsert_source_status(
    pool: &PgPool,
    source: &str,
    status: &str,
    error_ping_date: Option<NaiveDate>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO source_status (source, status, last_error_ping_date)
         VALUES ($1, $2, $3)
         ON CONFLICT (source) DO UPDATE SET
           status = EXCLUDED.status,
           last_error_ping_date = COALESCE(EXCLUDED.last_error_ping_date,
                                           source_status.last_error_ping_date)",
    )
    .bind(source)
    .bind(status)
    .bind(error_ping_date)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn all_source_statuses(pool: &PgPool) -> sqlx::Result<Vec<(String, String)>> {
    sqlx::query_as("SELECT source, status FROM source_status ORDER BY source")
        .fetch_all(pool)
        .await
}

pub async fn insert_check_run(
    pool: &PgPool,
    run_at: DateTime<Utc>,
    new_count: i32,
    total_count: i32,
) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO check_run (run_at, new_count, total_count) VALUES ($1, $2, $3)")
        .bind(run_at)
        .bind(new_count)
        .bind(total_count)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn latest_check_run(pool: &PgPool) -> sqlx::Result<Option<DateTime<Utc>>> {
    let row: Option<(DateTime<Utc>,)> =
        sqlx::query_as("SELECT run_at FROM check_run ORDER BY id DESC LIMIT 1")
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|r| r.0))
}

pub async fn insert_email_token(
    pool: &PgPool,
    email: &str,
    token: &str,
    expires_at: DateTime<Utc>,
) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO email_tokens (token, email, expires_at) VALUES ($1, $2, $3)")
        .bind(token)
        .bind(email)
        .bind(expires_at)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct EmailTokenState {
    pub email: String,
    pub used: bool,
}

pub async fn lookup_email_token(
    pool: &PgPool,
    token: &str,
) -> sqlx::Result<Option<EmailTokenState>> {
    let row: Option<(String, bool)> = sqlx::query_as(
        "SELECT email, used FROM email_tokens WHERE token = $1 AND expires_at > now()",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(email, used)| EmailTokenState { email, used }))
}

pub async fn consume_email_token(pool: &PgPool, token: &str) -> sqlx::Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "UPDATE email_tokens SET used = true WHERE token = $1 AND used = false AND expires_at > now()
         RETURNING email",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}

pub async fn find_or_create_user(
    pool: &PgPool,
    provider: &str,
    provider_id: &str,
    email: &str,
) -> sqlx::Result<i64> {
    let mut tx = pool.begin().await?;
    // existing identity?
    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT user_id FROM user_identities WHERE provider = $1 AND provider_id = $2",
    )
    .bind(provider)
    .bind(provider_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((uid,)) = existing {
        tx.commit().await?;
        return Ok(uid);
    }
    // existing user with this email? Skip the lookup when there is no email so
    // that an empty email never links distinct accounts together.
    let existing_user: Option<(i64,)> = if email.is_empty() {
        None
    } else {
        sqlx::query_as("SELECT id FROM users WHERE email = $1")
            .bind(email)
            .fetch_optional(&mut *tx)
            .await?
    };
    let user_id = match existing_user {
        Some((id,)) => id,
        None => {
            let row: (i64,) = sqlx::query_as("INSERT INTO users (email) VALUES ($1) RETURNING id")
                .bind(email)
                .fetch_one(&mut *tx)
                .await?;
            row.0
        }
    };
    // insert identity, tolerate a concurrent insert winning the PK
    sqlx::query(
        "INSERT INTO user_identities (user_id, provider, provider_id) VALUES ($1, $2, $3)
         ON CONFLICT (provider, provider_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(provider)
    .bind(provider_id)
    .execute(&mut *tx)
    .await?;
    // If the identity already existed from a concurrent request, return that user_id
    let identity_owner: Option<(i64,)> = sqlx::query_as(
        "SELECT user_id FROM user_identities WHERE provider = $1 AND provider_id = $2",
    )
    .bind(provider)
    .bind(provider_id)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(identity_owner.map(|r| r.0).unwrap_or(user_id))
}

pub async fn create_session(
    pool: &PgPool,
    user_id: i64,
    token: &str,
    expires_at: DateTime<Utc>,
) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO sessions (token, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(token)
        .bind(user_id)
        .bind(expires_at)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn lookup_session(pool: &PgPool, token: &str) -> sqlx::Result<Option<(i64, String)>> {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT s.user_id, u.email FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token = $1 AND s.expires_at > now()",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn delete_session(pool: &PgPool, token: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token = $1")
        .bind(token)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn prune_expired_sessions(pool: &PgPool) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM sessions WHERE expires_at <= now()")
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_ignored(
    pool: &PgPool,
    user_id: i64,
    cinema: &str,
    title: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO movie_ignore (user_id, cinema_id, title)
         VALUES ($1, (SELECT id FROM cinema WHERE name = $2), $3)
         ON CONFLICT (user_id, cinema_id, title) DO NOTHING",
    )
    .bind(user_id)
    .bind(cinema)
    .bind(title)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn unset_ignored(
    pool: &PgPool,
    user_id: i64,
    cinema: &str,
    title: &str,
) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM movie_ignore
 WHERE user_id = $1 AND cinema_id = (SELECT id FROM cinema WHERE name = $2) AND title = $3")
        .bind(user_id)
        .bind(cinema)
        .bind(title)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn ignored_keys(pool: &PgPool, user_id: i64) -> sqlx::Result<HashSet<(String, String)>> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT c.name, mi.title FROM movie_ignore mi JOIN cinema c ON c.id = mi.cinema_id WHERE mi.user_id = $1")
            .bind(user_id)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use chrono::TimeZone;

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 4, hour, 30, 0).unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn movie_upsert_updates_metadata(pool: PgPool) {
        let id1 = upsert_movie(
            &pool,
            "Cineplexx Linz",
            "F1",
            Some(100),
            &["Drama".into()],
            Some("https://p/1.jpg"),
            None,
        )
        .await
        .unwrap();
        let id2 = upsert_movie(
            &pool,
            "Cineplexx Linz",
            "F1",
            Some(120),
            &["Action".into()],
            None,
            Some("a.jpg"),
        )
        .await
        .unwrap();
        assert_eq!(id1, id2);
        let view = upcoming_view(&pool, Utc::now()).await.unwrap();
        assert!(view.is_empty()); // no showings yet
                                  // the second upsert must have overwritten the metadata
        assert!(
            insert_showing(&pool, id2, at(19), "OV", "Saal 6", "https://x", at(12))
                .await
                .unwrap()
                .is_some()
        );
        let view = upcoming_view(&pool, at(0)).await.unwrap();
        assert_eq!(view.len(), 1);
        assert_eq!(view[0].runtime_min, Some(120));
        assert_eq!(view[0].genres, vec!["Action"]);
        assert_eq!(view[0].poster_file.as_deref(), Some("a.jpg"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn showing_insert_dedups(pool: PgPool) {
        let mid = upsert_movie(&pool, "Cineplexx Linz", "F1", None, &[], None, None)
            .await
            .unwrap();
        assert!(
            insert_showing(&pool, mid, at(19), "OV", "Saal 6", "https://x", at(12))
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            insert_showing(&pool, mid, at(19), "OV", "Saal 6", "https://x", at(12))
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn view_joins_movie_metadata(pool: PgPool) {
        let mid = upsert_movie(
            &pool,
            "Cineplexx Linz",
            "F1",
            Some(100),
            &["Drama".into()],
            None,
            Some("a.jpg"),
        )
        .await
        .unwrap();
        assert!(
            insert_showing(&pool, mid, at(19), "OV", "Saal 6", "https://x", at(12))
                .await
                .unwrap()
                .is_some()
        );
        let view = upcoming_view(&pool, at(0)).await.unwrap();
        assert_eq!(view.len(), 1);
        assert_eq!(view[0].movie, "F1");
        assert_eq!(view[0].runtime_min, Some(100));
        assert_eq!(view[0].genres, vec!["Drama"]);
        assert_eq!(view[0].poster_file.as_deref(), Some("a.jpg"));
        // filtered by `since`
        assert!(upcoming_view(&pool, at(20)).await.unwrap().is_empty());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn prune_removes_old_showings_and_orphan_movies(pool: PgPool) {
        let mid = upsert_movie(&pool, "Cineplexx Linz", "F1", None, &[], None, None)
            .await
            .unwrap();
        assert!(
            insert_showing(&pool, mid, at(1), "OV", "", "https://x", at(0))
                .await
                .unwrap()
                .is_some()
        );
        prune(&pool, at(2)).await.unwrap();
        assert!(upcoming_view(&pool, at(0)).await.unwrap().is_empty());
        // movie is gone too -> re-insert gets a fresh id
        let mid2 = upsert_movie(&pool, "Cineplexx Linz", "F1", None, &[], None, None)
            .await
            .unwrap();
        assert_ne!(mid, mid2);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn source_status_roundtrip_and_ping_date_kept(pool: PgPool) {
        assert!(get_source_status(&pool, "megaplex")
            .await
            .unwrap()
            .is_none());
        upsert_source_status(
            &pool,
            "megaplex",
            "error",
            Some(NaiveDate::from_ymd_opt(2026, 8, 2).unwrap()),
        )
        .await
        .unwrap();
        // recover with ok: ping date must survive (rate limit still applies today)
        upsert_source_status(&pool, "megaplex", "ok", None)
            .await
            .unwrap();
        let (status, ping) = get_source_status(&pool, "megaplex").await.unwrap().unwrap();
        assert_eq!(status, "ok");
        assert_eq!(ping, Some(NaiveDate::from_ymd_opt(2026, 8, 2).unwrap()));
        let all = all_source_statuses(&pool).await.unwrap();
        assert_eq!(all, vec![("megaplex".to_string(), "ok".to_string())]);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn check_run_latest(pool: PgPool) {
        assert!(latest_check_run(&pool).await.unwrap().is_none());
        insert_check_run(&pool, at(1), 2, 5).await.unwrap();
        insert_check_run(&pool, at(2), 0, 3).await.unwrap();
        assert_eq!(latest_check_run(&pool).await.unwrap(), Some(at(2)));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn email_token_insert_and_consume(pool: PgPool) {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let token_bytes: [u8; 32] = rng.gen();
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);
        let expires = Utc::now() + chrono::Duration::minutes(15);
        insert_email_token(&pool, "a@b.com", &token, expires)
            .await
            .unwrap();
        let email = consume_email_token(&pool, &token).await.unwrap();
        assert_eq!(email, Some("a@b.com".into()));
        // second consumption fails (already used)
        let email2 = consume_email_token(&pool, &token).await.unwrap();
        assert_eq!(email2, None);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn lookup_email_token_states(pool: PgPool) {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let token_bytes: [u8; 32] = rng.gen();
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);
        let expires = Utc::now() + chrono::Duration::minutes(15);
        insert_email_token(&pool, "a@b.com", &token, expires)
            .await
            .unwrap();

        // not used yet
        let st = lookup_email_token(&pool, &token).await.unwrap().unwrap();
        assert_eq!(st.email, "a@b.com");
        assert!(!st.used);

        // after consumption, used=true
        let _ = consume_email_token(&pool, &token).await.unwrap();
        let st = lookup_email_token(&pool, &token).await.unwrap().unwrap();
        assert!(st.used);

        // unknown token -> None
        assert!(lookup_email_token(&pool, "nope").await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn lookup_email_token_expired_returns_none(pool: PgPool) {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let token_bytes: [u8; 32] = rng.gen();
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);
        let expires = Utc::now() - chrono::Duration::minutes(1);
        insert_email_token(&pool, "a@b.com", &token, expires)
            .await
            .unwrap();
        assert!(lookup_email_token(&pool, &token).await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn email_token_expired(pool: PgPool) {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let token_bytes: [u8; 32] = rng.gen();
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);
        let expires = Utc::now() - chrono::Duration::minutes(1);
        insert_email_token(&pool, "a@b.com", &token, expires)
            .await
            .unwrap();
        assert_eq!(consume_email_token(&pool, &token).await.unwrap(), None);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_or_create_user_new(pool: PgPool) {
        let uid = find_or_create_user(&pool, "email", "x@y.com", "x@y.com")
            .await
            .unwrap();
        assert!(uid > 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_or_create_user_existing_identity(pool: PgPool) {
        let uid1 = find_or_create_user(&pool, "google", "sub123", "a@b.com")
            .await
            .unwrap();
        let uid2 = find_or_create_user(&pool, "google", "sub123", "a@b.com")
            .await
            .unwrap();
        assert_eq!(uid1, uid2);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_or_create_user_link_by_email(pool: PgPool) {
        let uid1 = find_or_create_user(&pool, "google", "sub123", "a@b.com")
            .await
            .unwrap();
        // login via email with same email address should link to existing user
        let uid2 = find_or_create_user(&pool, "email", "a@b.com", "a@b.com")
            .await
            .unwrap();
        assert_eq!(uid1, uid2);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_or_create_user_empty_email_does_not_link(pool: PgPool) {
        let uid1 = find_or_create_user(&pool, "github", "user-1", "")
            .await
            .unwrap();
        let uid2 = find_or_create_user(&pool, "github", "user-2", "")
            .await
            .unwrap();
        assert_ne!(uid1, uid2);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn session_lifecycle(pool: PgPool) {
        let uid = find_or_create_user(&pool, "email", "a@b.com", "a@b.com")
            .await
            .unwrap();
        let token = "sess-token-abc";
        let expires = Utc::now() + chrono::Duration::days(30);
        create_session(&pool, uid, token, expires).await.unwrap();
        let found = lookup_session(&pool, token).await.unwrap();
        assert_eq!(found, Some((uid, "a@b.com".to_string())));
        delete_session(&pool, token).await.unwrap();
        assert_eq!(lookup_session(&pool, token).await.unwrap(), None);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn session_expired_not_found(pool: PgPool) {
        let uid = find_or_create_user(&pool, "email", "a@b.com", "a@b.com")
            .await
            .unwrap();
        let expires = Utc::now() - chrono::Duration::minutes(1);
        create_session(&pool, uid, "expired-token", expires)
            .await
            .unwrap();
        assert_eq!(lookup_session(&pool, "expired-token").await.unwrap(), None);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn prune_expired_sessions_works(pool: PgPool) {
        let uid = find_or_create_user(&pool, "email", "a@b.com", "a@b.com")
            .await
            .unwrap();
        let expires_old = Utc::now() - chrono::Duration::minutes(1);
        let expires_fresh = Utc::now() + chrono::Duration::days(1);
        create_session(&pool, uid, "old-sess", expires_old)
            .await
            .unwrap();
        create_session(&pool, uid, "fresh-sess", expires_fresh)
            .await
            .unwrap();
        prune_expired_sessions(&pool).await.unwrap();
        assert_eq!(lookup_session(&pool, "old-sess").await.unwrap(), None);
        assert!(lookup_session(&pool, "fresh-sess").await.unwrap().is_some());
    }
}
