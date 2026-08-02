use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;
use std::path::Path;

fn parse_dt(value: &serde_json::Value) -> Option<DateTime<Utc>> {
    value
        .as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
}

pub async fn run(pool: &PgPool, data_dir: &Path) -> anyhow::Result<()> {
    let payload: serde_json::Value = match std::fs::read_to_string(data_dir.join("showings.json")) {
        Ok(text) => serde_json::from_str(&text)?,
        Err(_) => {
            tracing::info!("no showings.json found, nothing to import");
            return Ok(());
        }
    };
    let state: serde_json::Value = std::fs::read_to_string(data_dir.join("state.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(serde_json::json!({}));
    let generated_at = parse_dt(&payload["generated_at"]).unwrap_or_else(Utc::now);
    let empty = serde_json::json!({});

    let showings = payload["showings"].as_array().cloned().unwrap_or_default();
    for s in &showings {
        let cinema = s["cinema"].as_str().unwrap_or_default();
        let movie = s["movie"].as_str().unwrap_or_default();
        let Some(start) = parse_dt(&s["start"]) else {
            continue;
        };
        let key = format!("{cinema}|{movie}");
        let meta = payload["movies"].get(&key).unwrap_or(&empty);
        let genres: Vec<String> = meta["genres"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|g| g.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let movie_id = crate::db::upsert_movie(
            pool,
            cinema,
            movie,
            meta["runtime_min"].as_i64().map(|v| v as i32),
            &genres,
            meta["poster"].as_str(),
            meta["poster_file"].as_str(),
        )
        .await?;
        let seen_key = format!("{key}|{}", s["start"].as_str().unwrap_or_default());
        let first_seen = parse_dt(&state["seen"][&seen_key]).unwrap_or(generated_at);
        crate::db::insert_showing(
            pool,
            movie_id,
            start,
            s["version"].as_str().unwrap_or_default(),
            s["hall"].as_str().unwrap_or_default(),
            s["url"].as_str().unwrap_or_default(),
            first_seen,
        )
        .await?;
    }

    if let Some(sources) = payload["sources"].as_object() {
        for (source, status) in sources {
            let ping = state["error_pings"][source]
                .as_str()
                .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
            crate::db::upsert_source_status(pool, source, status.as_str().unwrap_or("ok"), ping)
                .await?;
        }
    }
    crate::db::insert_check_run(pool, generated_at, 0, showings.len() as i32).await?;
    tracing::info!(imported = showings.len(), "state import finished");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, NaiveDate, Utc};

    fn write_state(dir: &Path) {
        std::fs::write(
            dir.join("showings.json"),
            serde_json::json!({
                "generated_at": "2026-08-01T12:00:00+02:00",
                "sources": {"cineplexx": "ok", "megaplex": "error"},
                "movies": {
                    "Cineplexx Linz|The Odyssey": {
                        "runtime_min": 180,
                        "genres": ["Abenteuer", "Historie"],
                        "poster": "https://x/p.jpg",
                        "poster_file": "a1b2.jpg"
                    }
                },
                "showings": [
                    {
                        "cinema": "Cineplexx Linz",
                        "movie": "The Odyssey",
                        "start": "2026-08-04T19:30:00+02:00",
                        "version": "OV",
                        "hall": "Saal 6",
                        "url": "https://cineplexx.at/film/die-odyssee"
                    }
                ]
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            dir.join("state.json"),
            serde_json::json!({
                "seen": {
                    "Cineplexx Linz|The Odyssey|2026-08-04T19:30:00+02:00": "2026-07-30T09:00:00+02:00"
                },
                "error_pings": {"megaplex": "2026-08-01"}
            })
            .to_string(),
        )
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn imports_json_state(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        write_state(dir.path());
        run(&pool, dir.path()).await.unwrap();

        let views = crate::db::upcoming_view(
            &pool,
            DateTime::parse_from_rfc3339("2026-08-01T00:00:00+00:00")
                .unwrap()
                .with_timezone(&Utc),
        )
        .await
        .unwrap();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].movie, "The Odyssey");
        assert_eq!(views[0].runtime_min, Some(180));
        assert_eq!(views[0].poster_file.as_deref(), Some("a1b2.jpg"));

        // dedup preserved: re-import inserts nothing new
        run(&pool, dir.path()).await.unwrap();
        let mid = crate::db::upsert_movie(
            &pool,
            "Cineplexx Linz",
            "The Odyssey",
            None,
            &[],
            None,
            None,
        )
        .await
        .unwrap();
        let inserted = crate::db::insert_showing(
            &pool,
            mid,
            DateTime::parse_from_rfc3339("2026-08-04T19:30:00+02:00")
                .unwrap()
                .with_timezone(&Utc),
            "OV",
            "Saal 6",
            "https://cineplexx.at/film/die-odyssee",
            Utc::now(),
        )
        .await
        .unwrap();
        assert!(!inserted, "imported showing must be treated as seen");

        // source statuses incl. error ping date
        let (status, ping) = crate::db::get_source_status(&pool, "megaplex")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status, "error");
        assert_eq!(ping, Some(NaiveDate::from_ymd_opt(2026, 8, 1).unwrap()));
        let (status, _) = crate::db::get_source_status(&pool, "cineplexx")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status, "ok");

        // check run seeded with generated_at
        let latest = crate::db::latest_check_run(&pool).await.unwrap().unwrap();
        assert_eq!(
            latest,
            DateTime::parse_from_rfc3339("2026-08-01T12:00:00+02:00")
                .unwrap()
                .with_timezone(&Utc)
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn missing_showings_json_is_a_noop(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        run(&pool, dir.path()).await.unwrap();
        assert!(crate::db::latest_check_run(&pool).await.unwrap().is_none());
    }
}
