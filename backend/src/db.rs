use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;

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
        "INSERT INTO movie (cinema, title, runtime_min, genres, poster_url, poster_file)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (cinema, title) DO UPDATE SET
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
) -> sqlx::Result<bool> {
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
    Ok(row.is_some())
}

pub async fn upcoming_view(pool: &PgPool, since: DateTime<Utc>) -> sqlx::Result<Vec<ShowingView>> {
    sqlx::query_as(
        "SELECT m.cinema, m.title AS movie, s.start, s.version, s.hall, s.url,
                m.runtime_min, m.genres, m.poster_file
         FROM showing s JOIN movie m ON m.id = s.movie_id
         WHERE s.start >= $1
         ORDER BY s.start, m.cinema",
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

#[cfg(test)]
mod tests {
    use super::*;
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
        insert_showing(&pool, id2, at(19), "OV", "Saal 6", "https://x", at(12))
            .await
            .unwrap();
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
        );
        assert!(
            !insert_showing(&pool, mid, at(19), "OV", "Saal 6", "https://x", at(12))
                .await
                .unwrap()
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
        insert_showing(&pool, mid, at(19), "OV", "Saal 6", "https://x", at(12))
            .await
            .unwrap();
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
        insert_showing(&pool, mid, at(1), "OV", "", "https://x", at(0))
            .await
            .unwrap();
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
}
