# Rust Backend + React SPA + Postgres Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Python/Flask OV watcher with a feature-parity rewrite: Rust (axum + sqlx + Postgres) backend, React/TypeScript SPA frontend, single-container deployment — per `docs/superpowers/specs/2026-08-02-rust-react-rewrite-design.md`.

**Architecture:** One Rust binary serves the JSON API, the built React statics, `/showings.ics`, `/posters/<name>`, `/healthz`, and runs the periodic check in a background tokio task. Postgres holds `movie` / `showing` / `source_status` / `check_run`; dedup is an `ON CONFLICT DO NOTHING` insert. Poster cache stays on disk under `DATA_DIR/posters/`. Module layout mirrors the current Python modules 1:1 so behavior is ported, not redesigned.

**Tech Stack:** Rust stable (2021 edition), axum 0.8, tokio 1, reqwest 0.12 (rustls), scraper 0.23, sqlx 0.8 (Postgres, runtime-checked queries, chrono), serde/serde_json 1, regex 1, chrono 0.4 + chrono-tz 0.10, sha1 0.10, thiserror 2, anyhow 1, async-trait 0.1, tracing 0.1. Frontend: React 19 + TypeScript + Vite 6, Vitest 3 + Testing Library. Postgres 17.

## Global Constraints

- **Behavior parity with the Python app.** Formats, wording, ordering and rounding ported exactly: Telegram message (German, `🎬 <b>Neue OV-Vorstellungen in Linz</b>`, weekday abbreviations `Mo Di Mi Do Fr Sa So`), web view model (English weekday abbreviations `Mon..Sun`, Megaplex listed first), ICS feed (RFC 5545, CRLF, 75-octet folding, `@ov-kino-linz` UIDs).
- All times stored as `TIMESTAMPTZ` / `DateTime<Utc>`; rendered in `Europe/Vienna` for display.
- ICS UID input string is `"{cinema}|{movie}|{start:%Y-%m-%dT%H:%M:%S%:z}"` with `start` rendered in Europe/Vienna — this reproduces the Python-era UIDs (no duplicate calendar events on cutover).
- Dedup key = `(movie_id, start)` unique constraint + `INSERT ... ON CONFLICT DO NOTHING`; rows actually inserted are the "new" showings.
- Pruning: showings with `start < now - 6h` are deleted each run, then orphan movies.
- Telegram error ping: at most one per source per calendar day (Vienna date), tracked in `source_status.last_error_ping_date`.
- No `unwrap()`/`panic!` in request/scheduler paths; errors become health statuses or logged failures. (`unwrap` is fine in tests and for `Selector::parse`/regex construction with literal patterns.)
- Fetchers are split into pure parse functions (`&str`/`serde_json::Value` → structs) plus thin async HTTP wrappers, so parser tests need no network.
- No new user-facing features beyond the spec (parity + Postgres + importer).
- Test DB: `docker compose up -d db` then `export DATABASE_URL=postgres://ov:ov@localhost:5432/ov` — required for `#[sqlx::test]` tests (it creates a fresh database per test).
- Commit style: plain imperative (e.g. `Add Cineplexx fetcher`), matching repo history.

---

### Task 1: Backend skeleton — Cargo project, config, `/healthz`

**Files:**
- Create: `backend/Cargo.toml`
- Create: `backend/src/main.rs`
- Create: `backend/src/config.rs`
- Create: `backend/src/web.rs`

**Interfaces:**
- Produces: `config::Config { telegram_token: Option<String>, telegram_chat_id: Option<String>, sources: Vec<String>, check_interval: Duration, data_dir: PathBuf, port: u16, database_url: String, static_dir: PathBuf }`
- Produces: `Config::from_env() -> anyhow::Result<Config>` and `Config::from_lookup(get: impl Fn(&str) -> Option<String>) -> anyhow::Result<Config>` (testable without touching process env)
- Produces: `web::healthz() -> &'static str` (a `Router` is added in Task 10)

- [ ] **Step 1: Create the Cargo project and lockfile**

```bash
mkdir -p backend/src
cat > backend/Cargo.toml <<'EOF'
[package]
name = "ov-watcher"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1"
async-trait = "0.1"
axum = "0.8"
chrono = { version = "0.4", features = ["serde"] }
chrono-tz = "0.10"
regex = "1"
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
scraper = "0.23"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha1 = "0.10"
sqlx = { version = "0.8", default-features = false, features = ["runtime-tokio-rustls", "postgres", "chrono", "migrate"] }
thiserror = "2"
tokio = { version = "1", features = ["full", "test-util"] }
tower-http = { version = "0.6", features = ["fs"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
tempfile = "3"
tower = { version = "0.5", features = ["util"] }
EOF
cargo generate-lockfile --manifest-path backend/Cargo.toml
echo '/target' > backend/.gitignore
```

- [ ] **Step 2: Write the failing config tests**

Create `backend/src/config.rs` with only the test module for now:

```rust
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub telegram_token: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub sources: Vec<String>,
    pub check_interval: Duration,
    pub data_dir: PathBuf,
    pub port: u16,
    pub database_url: String,
    pub static_dir: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> =
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |key| map.get(key).cloned()
    }

    #[test]
    fn defaults_when_only_database_url_set() {
        let cfg = Config::from_lookup(env_of(&[("DATABASE_URL", "postgres://x")])).unwrap();
        assert_eq!(cfg.sources, vec!["cineplexx", "megaplex"]);
        assert_eq!(cfg.check_interval, Duration::from_secs(3 * 3600));
        assert_eq!(cfg.data_dir, PathBuf::from("./data"));
        assert_eq!(cfg.static_dir, PathBuf::from("./frontend/dist"));
        assert_eq!(cfg.port, 8080);
        assert_eq!(cfg.telegram_token, None);
    }

    #[test]
    fn parses_all_overrides() {
        let cfg = Config::from_lookup(env_of(&[
            ("DATABASE_URL", "postgres://x"),
            ("TELEGRAM_BOT_TOKEN", "T"),
            ("TELEGRAM_CHAT_ID", "@ov_linz"),
            ("SOURCES", "cineplexx, megaplex,"),
            ("CHECK_INTERVAL_HOURS", "1.5"),
            ("DATA_DIR", "/data"),
            ("PORT", "9090"),
            ("STATIC_DIR", "/srv/static"),
        ]))
        .unwrap();
        assert_eq!(cfg.telegram_token.as_deref(), Some("T"));
        assert_eq!(cfg.telegram_chat_id.as_deref(), Some("@ov_linz"));
        assert_eq!(cfg.sources, vec!["cineplexx", "megaplex"]);
        assert_eq!(cfg.check_interval, Duration::from_secs(5400));
        assert_eq!(cfg.data_dir, PathBuf::from("/data"));
        assert_eq!(cfg.port, 9090);
    }

    #[test]
    fn missing_database_url_is_an_error() {
        assert!(Config::from_lookup(env_of(&[])).is_err());
    }

    #[test]
    fn invalid_port_is_an_error() {
        let cfg = Config::from_lookup(env_of(&[
            ("DATABASE_URL", "postgres://x"),
            ("PORT", "abc"),
        ]));
        assert!(cfg.is_err());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --manifest-path backend/Cargo.toml`
Expected: FAIL to compile — `Config::from_lookup` not found.

- [ ] **Step 4: Implement config, web stub, and main**

Append to `backend/src/config.rs` (inside the non-test part):

```rust
impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        let database_url = get("DATABASE_URL")
            .ok_or_else(|| anyhow::anyhow!("DATABASE_URL is required"))?;
        let sources = get("SOURCES")
            .unwrap_or_else(|| "cineplexx,megaplex".into())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let hours: f64 = get("CHECK_INTERVAL_HOURS")
            .unwrap_or_else(|| "3".into())
            .parse()
            .map_err(|_| anyhow::anyhow!("CHECK_INTERVAL_HOURS must be a number"))?;
        let port: u16 = get("PORT")
            .unwrap_or_else(|| "8080".into())
            .parse()
            .map_err(|_| anyhow::anyhow!("PORT must be a number"))?;
        Ok(Config {
            telegram_token: get("TELEGRAM_BOT_TOKEN"),
            telegram_chat_id: get("TELEGRAM_CHAT_ID"),
            sources,
            check_interval: Duration::from_secs_f64(hours * 3600.0),
            data_dir: PathBuf::from(get("DATA_DIR").unwrap_or_else(|| "./data".into())),
            port,
            database_url,
            static_dir: PathBuf::from(get("STATIC_DIR").unwrap_or_else(|| "./frontend/dist".into())),
        })
    }
}
```

Create `backend/src/web.rs`:

```rust
pub async fn healthz() -> &'static str {
    "ok"
}
```

Create `backend/src/main.rs`:

```rust
mod config;
mod web;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ov_watcher=info,tower_http=info".into()),
        )
        .init();
    let config = config::Config::from_env()?;
    tracing::info!(port = config.port, "config loaded");
    Ok(())
}
```

- [ ] **Step 5: Run tests, then commit**

Run: `cargo test --manifest-path backend/Cargo.toml`
Expected: 4 passed.

```bash
git add backend/
git commit -m "Add Rust backend skeleton with env config"
```

---

### Task 2: Postgres schema + `db.rs`

**Files:**
- Create: `docker-compose.yml`
- Create: `backend/migrations/0001_init.sql`
- Create: `backend/src/db.rs`
- Modify: `backend/src/main.rs` (add `mod db;`)

**Interfaces:**
- Consumes: `config::Config` (Task 1).
- Produces: `db::ShowingView { cinema: String, movie: String, start: DateTime<Utc>, version: String, hall: String, url: String, runtime_min: Option<i32>, genres: Vec<String>, poster_file: Option<String> }` (sqlx `FromRow`)
- Produces: `db::upsert_movie(pool, cinema: &str, title: &str, runtime_min: Option<i32>, genres: &[String], poster_url: Option<&str>, poster_file: Option<&str>) -> sqlx::Result<i64>`
- Produces: `db::insert_showing(pool, movie_id: i64, start: DateTime<Utc>, version: &str, hall: &str, url: &str, first_seen: DateTime<Utc>) -> sqlx::Result<bool>` — `true` = newly inserted
- Produces: `db::upcoming_view(pool, since: DateTime<Utc>) -> sqlx::Result<Vec<ShowingView>>`
- Produces: `db::prune(pool, cutoff: DateTime<Utc>) -> sqlx::Result<()>`
- Produces: `db::get_source_status(pool, source: &str) -> sqlx::Result<Option<(String, Option<NaiveDate>)>>` (status, last_error_ping_date)
- Produces: `db::upsert_source_status(pool, source: &str, status: &str, error_ping_date: Option<NaiveDate>) -> sqlx::Result<()>`
- Produces: `db::all_source_statuses(pool) -> sqlx::Result<Vec<(String, String)>>`
- Produces: `db::insert_check_run(pool, run_at: DateTime<Utc>, new_count: i32, total_count: i32) -> sqlx::Result<()>`
- Produces: `db::latest_check_run(pool) -> sqlx::Result<Option<DateTime<Utc>>>`

All functions accept `&sqlx::PgPool` as `pool` (Task 8's transaction is a sequence of these calls on the pool inside `pool.begin()` — for simplicity the checker uses the pool directly; each statement is atomic on its own).

- [ ] **Step 1: docker-compose with Postgres + migration SQL**

Create `docker-compose.yml`:

```yaml
services:
  db:
    image: postgres:17-alpine
    environment:
      POSTGRES_USER: ov
      POSTGRES_PASSWORD: ov
      POSTGRES_DB: ov
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U ov"]
      interval: 2s
      timeout: 3s
      retries: 20

volumes:
  pgdata: {}
```

Create `backend/migrations/0001_init.sql`:

```sql
CREATE TABLE movie (
  id          BIGSERIAL PRIMARY KEY,
  cinema      TEXT NOT NULL,
  title       TEXT NOT NULL,
  runtime_min INT,
  genres      TEXT[] NOT NULL DEFAULT '{}',
  poster_url  TEXT,
  poster_file TEXT,
  UNIQUE (cinema, title)
);

CREATE TABLE showing (
  id            BIGSERIAL PRIMARY KEY,
  movie_id      BIGINT NOT NULL REFERENCES movie(id) ON DELETE CASCADE,
  start         TIMESTAMPTZ NOT NULL,
  version       TEXT NOT NULL,
  hall          TEXT NOT NULL DEFAULT '',
  url           TEXT NOT NULL DEFAULT '',
  first_seen_at TIMESTAMPTZ NOT NULL,
  UNIQUE (movie_id, start)
);

CREATE TABLE source_status (
  source               TEXT PRIMARY KEY,
  status               TEXT NOT NULL,
  last_error_ping_date DATE
);

CREATE TABLE check_run (
  id          BIGSERIAL PRIMARY KEY,
  run_at      TIMESTAMPTZ NOT NULL,
  new_count   INT NOT NULL,
  total_count INT NOT NULL
);
```

- [ ] **Step 2: Write the failing db tests**

Start the test database first: `docker compose up -d db && export DATABASE_URL=postgres://ov:ov@localhost:5432/ov`

Create `backend/src/db.rs`:

```rust
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 4, hour, 30, 0).unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn movie_upsert_updates_metadata(pool: PgPool) {
        let id1 = upsert_movie(&pool, "Cineplexx Linz", "F1", Some(100), &["Drama".into()], Some("https://p/1.jpg"), None).await.unwrap();
        let id2 = upsert_movie(&pool, "Cineplexx Linz", "F1", Some(120), &["Action".into()], None, Some("a.jpg".into()).as_deref()).await.unwrap();
        assert_eq!(id1, id2);
        let view = upcoming_view(&pool, Utc::now()).await.unwrap();
        assert!(view.is_empty()); // no showings yet
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn showing_insert_dedups(pool: PgPool) {
        let mid = upsert_movie(&pool, "Cineplexx Linz", "F1", None, &[], None, None).await.unwrap();
        assert!(insert_showing(&pool, mid, at(19), "OV", "Saal 6", "https://x", at(12)).await.unwrap());
        assert!(!insert_showing(&pool, mid, at(19), "OV", "Saal 6", "https://x", at(12)).await.unwrap());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn view_joins_movie_metadata(pool: PgPool) {
        let mid = upsert_movie(&pool, "Cineplexx Linz", "F1", Some(100), &["Drama".into()], None, Some("a.jpg".into()).as_deref()).await.unwrap();
        insert_showing(&pool, mid, at(19), "OV", "Saal 6", "https://x", at(12)).await.unwrap();
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
        let mid = upsert_movie(&pool, "Cineplexx Linz", "F1", None, &[], None, None).await.unwrap();
        insert_showing(&pool, mid, at(1), "OV", "", "https://x", at(0)).await.unwrap();
        prune(&pool, at(2)).await.unwrap();
        assert!(upcoming_view(&pool, at(0)).await.unwrap().is_empty());
        // movie is gone too -> re-insert gets a fresh id
        let mid2 = upsert_movie(&pool, "Cineplexx Linz", "F1", None, &[], None, None).await.unwrap();
        assert_ne!(mid, mid2);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn source_status_roundtrip_and_ping_date_kept(pool: PgPool) {
        assert!(get_source_status(&pool, "megaplex").await.unwrap().is_none());
        upsert_source_status(&pool, "megaplex", "error", Some(NaiveDate::from_ymd_opt(2026, 8, 2).unwrap())).await.unwrap();
        // recover with ok: ping date must survive (rate limit still applies today)
        upsert_source_status(&pool, "megaplex", "ok", None).await.unwrap();
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
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --manifest-path backend/Cargo.toml`
Expected: FAIL to compile — functions not found.

- [ ] **Step 4: Implement the db functions**

Add to `backend/src/db.rs` (before the test module):

```rust
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
    sqlx::query("DELETE FROM movie m WHERE NOT EXISTS (SELECT 1 FROM showing s WHERE s.movie_id = m.id)")
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_source_status(
    pool: &PgPool,
    source: &str,
) -> sqlx::Result<Option<(String, Option<NaiveDate>)>> {
    sqlx::query_as(
        "SELECT status, last_error_ping_date FROM source_status WHERE source = $1",
    )
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
```

Register the module in `backend/src/main.rs` — change the module block to:

```rust
mod config;
mod db;
mod web;
```

- [ ] **Step 5: Run tests, then commit**

Run: `cargo test --manifest-path backend/Cargo.toml`
Expected: 4 (config) + 6 (db) passed.

```bash
git add backend/ docker-compose.yml
git commit -m "Add Postgres schema and db module"
```

---

### Task 3: `models.rs` — Showing, MovieMeta, OV matchers

**Files:**
- Create: `backend/src/models.rs`
- Modify: `backend/src/main.rs` (add `mod models;`)

**Interfaces:**
- Produces: `models::Showing { cinema: String, movie: String, start: DateTime<Utc>, version: String, hall: String, url: String }`, `Showing::key(&self) -> String` (`"Cinema|Movie|<start in Vienna, %Y-%m-%dT%H:%M:%S%:z>"`)
- Produces: `models::vienna_iso(start: DateTime<Utc>) -> String`
- Produces: `models::MovieMeta { runtime_min: Option<i32>, genres: Vec<String>, poster: Option<String> }` (implements `Default`)
- Produces: `models::is_english_ov_label(label: &str) -> bool`
- Produces: `models::cineplexx_session_version(session: &serde_json::Value) -> Option<String>`
- Produces: `models::megaplex_version(label: &str) -> Option<String>`

- [ ] **Step 1: Write the failing tests (port of `tests/test_models.py`)**

Create `backend/src/models.rs`:

```rust
use chrono::{DateTime, Utc};
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Showing {
    pub cinema: String,
    pub movie: String,
    pub start: DateTime<Utc>,
    pub version: String,
    pub hall: String,
    pub url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MovieMeta {
    pub runtime_min: Option<i32>,
    pub genres: Vec<String>,
    pub poster: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use chrono_tz::Europe::Vienna;
    use serde_json::json;

    fn make_showing() -> Showing {
        Showing {
            cinema: "Cineplexx Linz".into(),
            movie: "The Odyssey".into(),
            start: Vienna.with_ymd_and_hms(2026, 7, 20, 19, 0, 0).unwrap().with_timezone(&Utc),
            version: "OV".into(),
            hall: "Saal 6".into(),
            url: "https://cineplexx.at/film/die-odyssee".into(),
        }
    }

    #[test]
    fn showing_key_uses_vienna_iso() {
        let s = make_showing();
        assert_eq!(
            s.key(),
            "Cineplexx Linz|The Odyssey|2026-07-20T19:00:00+02:00"
        );
    }

    #[test]
    fn english_ov_labels() {
        assert!(is_english_ov_label("OV (Englisch)"));
        assert!(is_english_ov_label("OmU (Englisch)"));
        assert!(is_english_ov_label("OV"));
        assert!(is_english_ov_label("OmU"));
        assert!(!is_english_ov_label("2D"));
        assert!(!is_english_ov_label("IMAX"));
        assert!(!is_english_ov_label("OV (Französisch)"));
        assert!(!is_english_ov_label(""));
    }

    #[test]
    fn cineplexx_version_from_technologies() {
        let s = json!({"technologies": [["2D", "OV (Englisch)"], []], "conceptAttributesNames": ["OV"]});
        assert_eq!(cineplexx_session_version(&s).as_deref(), Some("OV"));
    }

    #[test]
    fn cineplexx_version_omu() {
        let s = json!({"technologies": [["2D", "OmU (Englisch)"], []], "conceptAttributesNames": []});
        assert_eq!(cineplexx_session_version(&s).as_deref(), Some("OmU"));
    }

    #[test]
    fn cineplexx_version_german_dub() {
        let s = json!({"technologies": [["2D"], []], "conceptAttributesNames": ["Wertvoll"]});
        assert_eq!(cineplexx_session_version(&s), None);
    }

    #[test]
    fn cineplexx_version_non_english_ov() {
        let s = json!({"technologies": [["2D", "OV (Französisch)"], []], "conceptAttributesNames": []});
        assert_eq!(cineplexx_session_version(&s), None);
    }

    #[test]
    fn megaplex_versions() {
        assert_eq!(megaplex_version("OV - IMAX 2D").as_deref(), Some("OV - IMAX 2D"));
        assert_eq!(megaplex_version("  OV - Dolby Vision 2D  ").as_deref(), Some("OV - Dolby Vision 2D"));
        assert_eq!(megaplex_version("Dolby Atmos 2D"), None);
        assert_eq!(megaplex_version("4DX 2D"), None);
    }

    #[test]
    fn movie_meta_defaults() {
        let m = MovieMeta::default();
        assert_eq!(m.runtime_min, None);
        assert!(m.genres.is_empty());
        assert_eq!(m.poster, None);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path backend/Cargo.toml models`
Expected: FAIL to compile — methods/functions not found.

- [ ] **Step 3: Implement the matchers**

Add to `backend/src/models.rs` (before the test module):

```rust
use regex::Regex;

static VERSION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(OV|OmU|OmdU)\b").unwrap());
static LANG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\(([^)]*)\)").unwrap());

impl Showing {
    pub fn key(&self) -> String {
        format!("{}|{}|{}", self.cinema, self.movie, vienna_iso(self.start))
    }
}

pub fn vienna_iso(start: DateTime<Utc>) -> String {
    start
        .with_timezone(&chrono_tz::Europe::Vienna)
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

/// True if a version label marks an English original version.
pub fn is_english_ov_label(label: &str) -> bool {
    if !VERSION_RE.is_match(label) {
        return false;
    }
    if let Some(lang) = LANG_RE.captures(label).and_then(|c| c.get(1)) {
        if !lang.as_str().to_lowercase().contains("englisch") {
            return false;
        }
    }
    true
}

/// 'OV'/'OmU'/'OmdU' for an English OV session, else None.
pub fn cineplexx_session_version(session: &serde_json::Value) -> Option<String> {
    if let Some(groups) = session.get("technologies").and_then(|t| t.as_array()) {
        for group in groups.iter().filter_map(|g| g.as_array()) {
            for label in group.iter().filter_map(|l| l.as_str()) {
                if VERSION_RE.is_match(label) && is_english_ov_label(label) {
                    return VERSION_RE
                        .captures(label)
                        .and_then(|c| c.get(1))
                        .map(|m| m.as_str().to_string());
                }
            }
        }
    }
    if let Some(attrs) = session
        .get("conceptAttributesNames")
        .and_then(|a| a.as_array())
    {
        for attr in attrs.iter().filter_map(|a| a.as_str()) {
            if matches!(attr, "OV" | "OmU" | "OmdU") {
                return Some(attr.to_string());
            }
        }
    }
    None
}

/// Megaplex tags original-language showings with a leading 'OV'.
pub fn megaplex_version(label: &str) -> Option<String> {
    let norm = label.split_whitespace().collect::<Vec<_>>().join(" ");
    norm.starts_with("OV").then_some(norm)
}
```

Register the module in `backend/src/main.rs`:

```rust
mod config;
mod db;
mod models;
mod web;
```

- [ ] **Step 4: Run tests, then commit**

Run: `cargo test --manifest-path backend/Cargo.toml`
Expected: all passed (10 total).

```bash
git add backend/
git commit -m "Add Showing/MovieMeta models and OV matchers"
```

---

### Task 4: HTTP client + Cineplexx fetcher

**Files:**
- Create: `backend/src/fetchers/mod.rs`
- Create: `backend/src/fetchers/cineplexx.rs`
- Modify: `backend/src/main.rs` (add `mod fetchers;`)

**Interfaces:**
- Consumes: `models::{Showing, MovieMeta, cineplexx_session_version}` (Task 3).
- Produces: `fetchers::SourceError` (thiserror enum, `Display` = message string; `#[error("{0}")]` variant `Msg(String)`)
- Produces: `fetchers::HttpClient { new(delay: Duration) -> Self }` with `get_json/get_text(url: &str, headers: &HeaderMap) -> Result<serde_json::Value/String, SourceError>` and `get_bytes(url: &str, headers: &HeaderMap) -> Result<bytes::Bytes, SourceError>` (20s timeout, sleeps `delay` after each request, non-2xx and network errors → `SourceError`)
- Produces: `fetchers::cineplexx::{CINEPLEXX_BASE, CINEPLEXX_CINEMA_NAME, CINEPLEXX_CINEMA_ID, USER_AGENT}`
- Produces: `fetchers::cineplexx::parse_cineplexx_showings(movies: &[serde_json::Value], sessions_by_movie: &HashMap<String, serde_json::Value>) -> (Vec<Showing>, HashMap<String, MovieMeta>)` — metas keyed by bare movie title (Python parity)
- Produces: `fetchers::cineplexx::fetch_cineplexx(http: &HttpClient) -> Result<(Vec<Showing>, HashMap<String, MovieMeta>), SourceError>` — metas keyed `"Cineplexx Linz|Title"`

- [ ] **Step 1: Write the failing tests**

Create `backend/src/fetchers/mod.rs`:

```rust
pub mod cineplexx;

use reqwest::header::HeaderMap;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("{0}")]
    Msg(String),
}

impl SourceError {
    pub fn msg(text: impl Into<String>) -> Self {
        SourceError::Msg(text.into())
    }
}

#[derive(Clone)]
pub struct HttpClient {
    client: reqwest::Client,
    delay: Duration,
}

#[cfg(test)]
pub(crate) fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures")
        .join(name);
    std::fs::read_to_string(path).unwrap()
}

#[cfg(test)]
mod tests {
    use super::cineplexx::*;
    use super::*;
    use std::collections::HashMap;

    fn load() -> (Vec<serde_json::Value>, HashMap<String, serde_json::Value>) {
        let movies: serde_json::Value =
            serde_json::from_str(&fixture("cineplexx_movies.json")).unwrap();
        let sessions: serde_json::Value =
            serde_json::from_str(&fixture("cineplexx_sessions_odyssey.json")).unwrap();
        (
            movies.as_array().unwrap().clone(),
            HashMap::from([("HO00016814".to_string(), sessions)]),
        )
    }

    #[test]
    fn finds_only_ov_sessions_at_linz() {
        let (movies, sessions) = load();
        let (showings, _) = parse_cineplexx_showings(&movies, &sessions);
        assert_eq!(showings.len(), 6);
        assert!(showings.iter().all(|s| s.version == "OV"));
        assert!(showings.iter().all(|s| s.cinema == "Cineplexx Linz"));
    }

    #[test]
    fn showing_fields() {
        let (movies, sessions) = load();
        let (showings, _) = parse_cineplexx_showings(&movies, &sessions);
        let s = &showings[0];
        assert_eq!(s.movie, "The Odyssey"); // leading '*' stripped
        assert_eq!(s.url, "https://cineplexx.at/film/die-odyssee");
        assert!(!s.hall.is_empty());
        let days: std::collections::HashSet<u32> = showings
            .iter()
            .map(|x| x.start.with_timezone(&chrono_tz::Europe::Vienna).day())
            .collect();
        assert_eq!(
            days,
            std::collections::HashSet::from([20, 21, 22, 23, 24, 26])
        );
    }

    #[test]
    fn extracts_movie_metadata() {
        let (movies, sessions) = load();
        let (_, metas) = parse_cineplexx_showings(&movies, &sessions);
        let m = &metas["The Odyssey"];
        assert_eq!(m.runtime_min, Some(180));
        assert_eq!(m.genres, vec!["Abenteuer", "Historie"]);
        assert!(m.poster.as_deref().unwrap_or("").starts_with("https://"));
    }

    #[test]
    fn metas_cover_all_movies_even_without_ov_sessions() {
        let (movies, sessions) = load();
        let (_, metas) = parse_cineplexx_showings(&movies, &sessions);
        assert_eq!(metas.len(), movies.len());
        assert_eq!(metas.len(), 17);
    }

    #[test]
    fn meta_edge_values() {
        let movies = vec![serde_json::json!({
            "id": "X1", "title": "Odd", "runTime": 0,
            "genres": null, "posterImage": ""
        })];
        let (_, metas) = parse_cineplexx_showings(&movies, &HashMap::new());
        assert_eq!(
            metas["Odd"],
            crate::models::MovieMeta { runtime_min: None, genres: vec![], poster: None }
        );
    }
}
```

Add `use chrono::Datelike;` at the top of the test module (needed for `.day()`).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path backend/Cargo.toml fetchers`
Expected: FAIL to compile — `HttpClient::new`, `parse_cineplexx_showings` not found. (Also add `mod fetchers;` to `main.rs` so the module builds.)

- [ ] **Step 3: Implement HttpClient + Cineplexx fetcher**

Append to `backend/src/fetchers/mod.rs` (replacing the `HttpClient` struct stub — implement its methods):

```rust
impl HttpClient {
    pub fn new(delay: Duration) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .expect("reqwest client");
        HttpClient { client, delay }
    }

    async fn get(&self, url: &str, headers: &HeaderMap) -> Result<reqwest::Response, SourceError> {
        let resp = self
            .client
            .get(url)
            .headers(headers.clone())
            .send()
            .await
            .map_err(|e| SourceError::msg(format!("GET {url} failed: {e}")))?
            .error_for_status()
            .map_err(|e| SourceError::msg(format!("GET {url} failed: {e}")))?;
        if self.delay > Duration::ZERO {
            tokio::time::sleep(self.delay).await;
        }
        Ok(resp)
    }

    pub async fn get_json(
        &self,
        url: &str,
        headers: &HeaderMap,
    ) -> Result<serde_json::Value, SourceError> {
        self.get(url, headers)
            .await?
            .json()
            .await
            .map_err(|_| SourceError::msg(format!("no JSON from {url}")))
    }

    pub async fn get_text(&self, url: &str, headers: &HeaderMap) -> Result<String, SourceError> {
        self.get(url, headers)
            .await?
            .text()
            .await
            .map_err(|e| SourceError::msg(format!("GET {url} failed: {e}")))
    }

    pub async fn get_bytes(
        &self,
        url: &str,
        headers: &HeaderMap,
    ) -> Result<bytes::Bytes, SourceError> {
        self.get(url, headers)
            .await?
            .bytes()
            .await
            .map_err(|e| SourceError::msg(format!("GET {url} failed: {e}")))
    }
}
```

(`bytes::Bytes` — add `bytes = "1"` to `[dependencies]` in `backend/Cargo.toml`.)

Create `backend/src/fetchers/cineplexx.rs`:

```rust
use super::{HttpClient, SourceError};
use crate::models::{cineplexx_session_version, MovieMeta, Showing};
use chrono::{DateTime, Utc};
use reqwest::header::{self, HeaderMap, HeaderValue};
use std::collections::HashMap;

pub const CINEPLEXX_BASE: &str = "https://app.cineplexx.at";
pub const CINEPLEXX_CINEMA_ID: &str = "1014";
pub const CINEPLEXX_CINEMA_NAME: &str = "Cineplexx Linz";
pub const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36";

fn headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("CINEPLEXX-Platform", HeaderValue::from_static("WEB"));
    h.insert(
        "client-key",
        HeaderValue::from_static("308330b1-52a5-4883-aee3-304240c22ea1"),
    );
    h.insert(header::USER_AGENT, HeaderValue::from_static(USER_AGENT));
    h
}

pub async fn fetch_cineplexx(
    http: &HttpClient,
) -> Result<(Vec<Showing>, HashMap<String, MovieMeta>), SourceError> {
    let headers = headers();
    let url = format!("{CINEPLEXX_BASE}/api/v1/cinemasweb/{CINEPLEXX_CINEMA_ID}/movies?date=all");
    let movies = http.get_json(&url, &headers).await?;
    let movies_list = movies
        .as_array()
        .filter(|m| !m.is_empty())
        .ok_or_else(|| SourceError::msg("Cineplexx: empty or invalid movie list"))?;
    let mut sessions_by_movie = HashMap::new();
    for movie in movies_list {
        let id = movie
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let url = format!("{CINEPLEXX_BASE}/api/v2/moviesweb/{id}/sessions?location=AUT");
        let data = http.get_json(&url, &headers).await?;
        if !data.is_array() {
            return Err(SourceError::msg(format!("Cineplexx: invalid sessions for {id}")));
        }
        sessions_by_movie.insert(id, data);
    }
    let (showings, metas) = parse_cineplexx_showings(movies_list, &sessions_by_movie);
    Ok((
        showings,
        metas
            .into_iter()
            .map(|(title, meta)| (format!("{CINEPLEXX_CINEMA_NAME}|{title}"), meta))
            .collect(),
    ))
}

pub fn parse_cineplexx_showings(
    movies: &[serde_json::Value],
    sessions_by_movie: &HashMap<String, serde_json::Value>,
) -> (Vec<Showing>, HashMap<String, MovieMeta>) {
    let mut showings = Vec::new();
    let mut metas: HashMap<String, MovieMeta> = HashMap::new();
    for movie in movies {
        let title = movie
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim_start_matches('*')
            .trim()
            .to_string();
        metas.entry(title.clone()).or_insert_with(|| cineplexx_meta(movie));
        let url = format!(
            "https://cineplexx.at/film/{}",
            movie.get("shortURL").and_then(|v| v.as_str()).unwrap_or("")
        );
        let id = movie.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let Some(groups) = sessions_by_movie.get(id).and_then(|v| v.as_array()) else {
            continue;
        };
        for group in groups {
            let Some(sessions) = group.get("sessions").and_then(|s| s.as_array()) else {
                continue;
            };
            for session in sessions {
                if session.get("cinemaId").and_then(|v| v.as_str()) != Some(CINEPLEXX_CINEMA_ID) {
                    continue;
                }
                let Some(version) = cineplexx_session_version(session) else {
                    continue;
                };
                let Some(showtime) = session.get("showtime").and_then(|v| v.as_str()) else {
                    continue;
                };
                let Ok(start) = DateTime::parse_from_rfc3339(showtime) else {
                    continue;
                };
                showings.push(Showing {
                    cinema: CINEPLEXX_CINEMA_NAME.to_string(),
                    movie: title.clone(),
                    start: start.with_timezone(&Utc),
                    version,
                    hall: session
                        .get("screenName")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    url: url.clone(),
                });
            }
        }
    }
    showings.sort_by_key(|s| s.start);
    (showings, metas)
}

fn cineplexx_meta(movie: &serde_json::Value) -> MovieMeta {
    let runtime_min = movie
        .get("runTime")
        .and_then(|v| v.as_i64())
        .filter(|&r| r != 0)
        .map(|r| r as i32);
    let genres = movie
        .get("genres")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|g| g.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let poster = movie
        .get("posterImage")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);
    MovieMeta { runtime_min, genres, poster }
}
```

Register the module in `backend/src/main.rs`:

```rust
mod config;
mod db;
mod fetchers;
mod models;
mod web;
```

- [ ] **Step 4: Run tests, then commit**

Run: `cargo test --manifest-path backend/Cargo.toml`
Expected: all passed (15 total).

```bash
git add backend/
git commit -m "Add HTTP client and Cineplexx fetcher"
```

---

### Task 5: Megaplex fetcher

**Files:**
- Create: `backend/src/fetchers/megaplex.rs`
- Modify: `backend/src/fetchers/mod.rs` (add `pub mod megaplex;`)

**Interfaces:**
- Consumes: `fetchers::{HttpClient, SourceError, fixture}` and `models::{Showing, MovieMeta, megaplex_version}` (Tasks 3–4).
- Produces: `fetchers::megaplex::{MEGAPLEX_BASE, MEGAPLEX_CINEMA_NAME, MEGAPLEX_DAYS (= 14)}`
- Produces: `fetchers::megaplex::parse_megaplex_ov_links(html: &str) -> Vec<String>`
- Produces: `fetchers::megaplex::parse_day(label: &str, today: NaiveDate) -> Option<NaiveDate>`
- Produces: `fetchers::megaplex::parse_megaplex_film_page(html: &str, url: &str, today: NaiveDate) -> Result<(Vec<Showing>, HashMap<String, MovieMeta>), SourceError>` — metas keyed by bare title
- Produces: `fetchers::megaplex::fetch_megaplex(http: &HttpClient, today: NaiveDate) -> Result<(Vec<Showing>, HashMap<String, MovieMeta>), SourceError>` — metas keyed `"Megaplex PlusCity|Title"`

- [ ] **Step 1: Write the failing tests (port of `tests/test_fetchers_megaplex.py`)**

Add to the `tests` module in `backend/src/fetchers/mod.rs`:

```rust
    mod megaplex_tests {
        use super::super::megaplex::*;
        use super::super::fixture;
        use chrono::NaiveDate;
        use chrono::Datelike;

        fn today() -> NaiveDate {
            NaiveDate::from_ymd_opt(2026, 7, 18).unwrap()
        }

        #[test]
        fn parse_ov_links_unique_and_absolute() {
            let html = fixture("megaplex_ov_program.html");
            let links = parse_megaplex_ov_links(&html);
            assert_eq!(
                links,
                vec![
                    format!("{MEGAPLEX_BASE}/film/linz/die-odyssee/ov"),
                    format!("{MEGAPLEX_BASE}/film/linz/insekten/ov"),
                    format!("{MEGAPLEX_BASE}/film/linz/vaiana/ov"),
                ]
            );
        }

        #[test]
        fn parse_film_page_showings() {
            let html = fixture("megaplex_film_ov.html");
            let url = format!("{MEGAPLEX_BASE}/film/linz/die-odyssee/ov");
            let (showings, _) = parse_megaplex_film_page(&html, &url, today()).unwrap();
            assert_eq!(showings.len(), 8);
            assert!(showings.iter().all(|s| s.cinema == "Megaplex PlusCity"));
            assert!(showings.iter().all(|s| s.movie == "Die Odyssee"));
            assert!(showings.iter().all(|s| s.version.starts_with("OV")));
        }

        #[test]
        fn parse_film_page_dates_and_links() {
            let html = fixture("megaplex_film_ov.html");
            let url = format!("{MEGAPLEX_BASE}/film/linz/die-odyssee/ov");
            let (showings, _) = parse_megaplex_film_page(&html, &url, today()).unwrap();
            let first = &showings[0];
            let local = first.start.with_timezone(&chrono_tz::Europe::Vienna);
            assert_eq!(local.day(), 18);
            assert_eq!((local.hour(), local.minute()), (19, 30));
            assert_eq!(first.version, "OV - Dolby Vision 2D");
            assert_eq!(first.url, format!("{MEGAPLEX_BASE}/ticket/57419/539128"));
            let mut days: Vec<u32> = showings
                .iter()
                .map(|s| s.start.with_timezone(&chrono_tz::Europe::Vienna).day())
                .collect();
            days.sort();
            assert_eq!(days, vec![18, 18, 19, 20, 21, 22, 23, 28]);
        }

        #[test]
        fn parse_film_page_metadata() {
            let html = fixture("megaplex_film_ov.html");
            let url = format!("{MEGAPLEX_BASE}/film/linz/die-odyssee/ov");
            let (_, metas) = parse_megaplex_film_page(&html, &url, today()).unwrap();
            let m = &metas["Die Odyssee"];
            assert_eq!(m.runtime_min, Some(173)); // JSON-LD duration "PT173M"
            assert_eq!(m.genres, vec!["Drama", "Action", "Abenteuer", "Fantasy"]);
            assert_eq!(
                m.poster.as_deref(),
                Some("https://megaplexog.s3.eu-north-1.amazonaws.com/Odysee1.webp")
            );
        }

        #[test]
        fn parse_film_page_without_jsonld_has_no_meta() {
            let html = "<html><body><h1>Other (Pluscity) - OV</h1>Aktuelles Kinoprogramm</body></html>";
            let (showings, metas) = parse_megaplex_film_page(html, "https://x", today()).unwrap();
            assert!(showings.is_empty());
            assert!(metas.is_empty());
        }

        #[test]
        fn parse_film_page_without_kinoprogramm_is_source_error() {
            let r = parse_megaplex_film_page("<html><body>garbage</body></html>", "https://x", today());
            assert!(r.is_err());
        }

        #[test]
        fn parse_day_labels() {
            let t = today();
            assert_eq!(parse_day("Heute", t), Some(t));
            assert_eq!(parse_day("Morgen", t), t.succ_opt());
            assert_eq!(
                parse_day("Montag, 20.07.2026", t),
                NaiveDate::from_ymd_opt(2026, 7, 20)
            );
            assert_eq!(parse_day("unrelated", t), None);
        }
    }
```

Add `use chrono::Timelike;` alongside `Datelike` in the megaplex test module (needed for `.hour()`/`.minute()`).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path backend/Cargo.toml megaplex`
Expected: FAIL to compile — module/functions not found.

- [ ] **Step 3: Implement the Megaplex fetcher**

Add `pub mod megaplex;` next to `pub mod cineplexx;` in `backend/src/fetchers/mod.rs`.

Create `backend/src/fetchers/megaplex.rs`:

```rust
use super::{HttpClient, SourceError};
use crate::models::{megaplex_version, MovieMeta, Showing};
use chrono::{Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Europe::Vienna;
use regex::Regex;
use reqwest::header::{self, HeaderMap, HeaderValue};
use scraper::{ElementRef, Html, Selector};
use std::collections::HashMap;
use std::sync::LazyLock;

pub const MEGAPLEX_BASE: &str = "https://www.megaplex.at";
pub const MEGAPLEX_CINEMA_NAME: &str = "Megaplex PlusCity";
pub const MEGAPLEX_DAYS: i64 = 14;

static OV_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/film/linz/[^/]+/ov$").unwrap());
static DAY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d{2})\.(\d{2})\.(\d{4})").unwrap());
static TIME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d{1,2}):(\d{2})").unwrap());
static LD_DURATION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^PT(?:(\d+)H)?(?:(\d+)M)?$").unwrap());
static TITLE_SPLIT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s*\(Pluscity\)|\s+-\s+OV").unwrap());

fn headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(
        header::USER_AGENT,
        HeaderValue::from_static(super::cineplexx::USER_AGENT),
    );
    h
}

/// Element text like BeautifulSoup's get_text(" ", strip=True).
fn text_of(el: &ElementRef) -> String {
    el.text()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn parse_megaplex_ov_links(html: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse("a[href]").unwrap();
    let mut links: Vec<String> = Vec::new();
    for a in doc.select(&sel) {
        if let Some(href) = a.value().attr("href") {
            if OV_LINK_RE.is_match(href) {
                let url = format!("{MEGAPLEX_BASE}{href}");
                if !links.contains(&url) {
                    links.push(url);
                }
            }
        }
    }
    links
}

pub fn parse_day(label: &str, today: NaiveDate) -> Option<NaiveDate> {
    let norm = label.split_whitespace().collect::<Vec<_>>().join(" ");
    if norm == "Heute" {
        return Some(today);
    }
    if norm == "Morgen" {
        return today.succ_opt();
    }
    DAY_RE.captures(&norm).and_then(|c| {
        NaiveDate::from_ymd_opt(
            c[3].parse().ok()?,
            c[2].parse().ok()?,
            c[1].parse().ok()?,
        )
    })
}

fn string_or_array(value: Option<&serde_json::Value>) -> Vec<String> {
    match value {
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        Some(serde_json::Value::Array(a)) => {
            a.iter().filter_map(|v| v.as_str().map(String::from)).collect()
        }
        _ => vec![],
    }
}

fn jsonld_meta(doc: &Html) -> Option<MovieMeta> {
    let sel = Selector::parse(r#"script[type="application/ld+json"]"#).unwrap();
    for tag in doc.select(&sel) {
        let content: String = tag.text().collect();
        let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };
        let blocks: Vec<&serde_json::Value> = match data.as_array() {
            Some(a) => a.iter().collect(),
            None => vec![&data],
        };
        for block in blocks {
            if block.get("@type").and_then(|v| v.as_str()) != Some("Movie") {
                continue;
            }
            let runtime_min = block
                .get("duration")
                .and_then(|v| v.as_str())
                .and_then(|d| {
                    let c = LD_DURATION_RE.captures(d)?;
                    let h: i32 = c.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
                    let m: i32 = c.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
                    Some(h * 60 + m).filter(|&r| r != 0)
                });
            let genres = string_or_array(block.get("genre"));
            let images = string_or_array(block.get("image"));
            return Some(MovieMeta {
                runtime_min,
                genres,
                poster: images.into_iter().next(),
            });
        }
    }
    None
}

pub fn parse_megaplex_film_page(
    html: &str,
    url: &str,
    today: NaiveDate,
) -> Result<(Vec<Showing>, HashMap<String, MovieMeta>), SourceError> {
    let doc = Html::parse_document(html);
    let all_text: String = doc.root_element().text().collect();
    if !all_text.contains("Kinoprogramm") {
        return Err(SourceError::msg(format!("unexpected Megaplex film page: {url}")));
    }
    let h1_sel = Selector::parse("h1").unwrap();
    let title = doc
        .select(&h1_sel)
        .next()
        .map(|h| {
            TITLE_SPLIT_RE
                .split(&text_of(&h))
                .next()
                .unwrap_or("")
                .trim()
                .to_string()
        })
        .unwrap_or_default();
    let mut metas = HashMap::new();
    if let Some(meta) = jsonld_meta(&doc) {
        if !title.is_empty() {
            metas.insert(title.clone(), meta);
        }
    }
    let day_sel = Selector::parse("div.day-group").unwrap();
    let h3_sel = Selector::parse("h3").unwrap();
    let link_sel = Selector::parse("a.card-highlights-link").unwrap();
    let label_sel = Selector::parse(".card-highlights-content-time-kino").unwrap();
    let mut showings = Vec::new();
    for group in doc.select(&day_sel) {
        let Some(day) = group
            .select(&h3_sel)
            .next()
            .and_then(|h| parse_day(&text_of(&h), today))
        else {
            continue;
        };
        for a in group.select(&link_sel) {
            let Some(version) = a
                .select(&label_sel)
                .next()
                .and_then(|el| megaplex_version(&text_of(&el)))
            else {
                continue;
            };
            let Some(tm) = TIME_RE.captures(a.value().attr("title").unwrap_or("")) else {
                continue;
            };
            let Some(naive) =
                day.and_hms_opt(tm[1].parse().unwrap_or(0), tm[2].parse().unwrap_or(0), 0)
            else {
                continue;
            };
            let Some(local) = Vienna.from_local_datetime(&naive).single() else {
                continue;
            };
            let href = a.value().attr("href").unwrap_or("");
            let full_url = if href.starts_with('/') {
                format!("{MEGAPLEX_BASE}{href}")
            } else {
                href.to_string()
            };
            showings.push(Showing {
                cinema: MEGAPLEX_CINEMA_NAME.to_string(),
                movie: title.clone(),
                start: local.with_timezone(&Utc),
                version,
                hall: String::new(),
                url: full_url,
            });
        }
    }
    showings.sort_by_key(|s| s.start);
    Ok((showings, metas))
}

pub async fn fetch_megaplex(
    http: &HttpClient,
    today: NaiveDate,
) -> Result<(Vec<Showing>, HashMap<String, MovieMeta>), SourceError> {
    let headers = headers();
    let mut links: Vec<String> = Vec::new();
    for i in 0..MEGAPLEX_DAYS {
        let day = today + Duration::days(i);
        let html = http
            .get_text(&format!("{MEGAPLEX_BASE}/kinoprogramm/linz/{day}/ov"), &headers)
            .await?;
        if !html.contains("Kinoprogramm") {
            return Err(SourceError::msg(format!(
                "Megaplex: unexpected program page for {day}"
            )));
        }
        for url in parse_megaplex_ov_links(&html) {
            if !links.contains(&url) {
                links.push(url);
            }
        }
    }
    let mut showings = Vec::new();
    let mut metas: HashMap<String, MovieMeta> = HashMap::new();
    for url in links {
        let html = http.get_text(&url, &headers).await?;
        let (page_showings, page_metas) = parse_megaplex_film_page(&html, &url, today)?;
        showings.extend(page_showings);
        for (title, meta) in page_metas {
            metas
                .entry(format!("{MEGAPLEX_CINEMA_NAME}|{title}"))
                .or_insert(meta);
        }
    }
    Ok((showings, metas))
}
```

- [ ] **Step 4: Run tests, then commit**

Run: `cargo test --manifest-path backend/Cargo.toml`
Expected: all passed (22 total).

```bash
git add backend/
git commit -m "Add Megaplex fetcher"
```

---

### Task 6: `notify.rs` — Telegram formatting, chunking, sender

**Files:**
- Create: `backend/src/notify.rs`
- Modify: `backend/src/main.rs` (add `mod notify;`)

**Interfaces:**
- Consumes: `models::{Showing, MovieMeta}` (Task 3).
- Produces: `notify::escape_html(s: &str) -> String` (`& < >`) and `notify::escape_attr(s: &str) -> String` (additionally `"` → `&quot;`, `'` → `&#x27;` — Python `html.escape` parity)
- Produces: `notify::format_message(showings: &[Showing], movies: &HashMap<String, MovieMeta>) -> String`
- Produces: `notify::format_error(source: &str, error: &dyn Display) -> String`
- Produces: `notify::chunk_text(text: &str, limit: usize) -> Vec<String>` (char-based, line boundaries, hard-wrap fallback)
- Produces: `notify::MAX_LEN (= 4096)`
- Produces: `notify::Notifier` trait (`#[async_trait] async fn send(&self, text: &str) -> anyhow::Result<()>`) and `notify::TelegramNotifier::new(token: &str, chat_id: &str) -> Self` (+ `with_base_url` for tests)

- [ ] **Step 1: Write the failing tests (port of `tests/test_notify.py`)**

Create `backend/src/notify.rs`:

```rust
use crate::models::{MovieMeta, Showing};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

pub const MAX_LEN: usize = 4096;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use chrono_tz::Europe::Vienna;

    fn make(cinema: &str, movie: &str, day: u32, hour: u32, minute: u32, version: &str, hall: &str, url: &str) -> Showing {
        Showing {
            cinema: cinema.into(),
            movie: movie.into(),
            start: Vienna.with_ymd_and_hms(2026, 7, day, hour, minute, 0).unwrap().with_timezone(&Utc),
            version: version.into(),
            hall: hall.into(),
            url: url.into(),
        }
    }

    #[test]
    fn groups_showings_under_movie_titles() {
        let showings = vec![
            make("Megaplex PlusCity", "Die Odyssee", 20, 19, 45, "OV - IMAX 2D", "", "https://www.megaplex.at/ticket/57419/539128"),
            make("Cineplexx Linz", "The Odyssey", 21, 20, 15, "OV", "Saal 3", "https://cineplexx.at/film/die-odyssee"),
            make("Cineplexx Linz", "The Odyssey", 20, 19, 0, "OV", "Saal 6", "https://cineplexx.at/film/die-odyssee"),
        ];
        let msg = format_message(&showings, &HashMap::new());
        let lines: Vec<&str> = msg.split('\n').collect();
        assert_eq!(lines[0], "🎬 <b>Neue OV-Vorstellungen in Linz</b>");
        let pos = |needle: &str| lines.iter().position(|l| *l == needle).unwrap();
        assert!(pos("<b>Cineplexx Linz</b>") < pos("<b>Megaplex PlusCity</b>"));
        assert_eq!(msg.matches("<b>The Odyssey (OV)</b>").count(), 1);
        let monday = "• <a href=\"https://cineplexx.at/film/die-odyssee\">Saal 6 · Mo 20.07., 19:00</a>";
        let tuesday = "• <a href=\"https://cineplexx.at/film/die-odyssee\">Saal 3 · Di 21.07., 20:15</a>";
        assert!(msg.contains(monday) && msg.contains(tuesday));
        assert!(pos(monday) < pos(tuesday));
        assert!(msg.contains("• <a href=\"https://www.megaplex.at/ticket/57419/539128\">Mo 20.07., 19:45</a>"));
        assert!(!lines.iter().any(|l| l.starts_with("http")));
    }

    #[test]
    fn version_on_lines_when_versions_differ() {
        let showings = vec![
            make("Cineplexx Linz", "F1", 20, 19, 0, "OV", "Saal 6", "https://x/1"),
            make("Cineplexx Linz", "F1", 22, 18, 30, "OmU", "Saal 1", "https://x/2"),
        ];
        let msg = format_message(&showings, &HashMap::new());
        assert!(msg.contains("<b>F1</b>"));
        assert!(msg.contains("• <a href=\"https://x/1\">Saal 6 · Mo 20.07., 19:00 · OV</a>"));
        assert!(msg.contains("• <a href=\"https://x/2\">Saal 1 · Mi 22.07., 18:30 · OmU</a>"));
    }

    #[test]
    fn escapes_html() {
        let showings = vec![make("Cineplexx Linz", "Fast & Furious <Final>", 20, 20, 0, "OV", "", "https://x.at/film?a=1&b=2")];
        let msg = format_message(&showings, &HashMap::new());
        assert!(msg.contains("<b>Fast &amp; Furious &lt;Final&gt; (OV)</b>"));
        assert!(msg.contains("href=\"https://x.at/film?a=1&amp;b=2\""));
        assert!(!msg.contains("Fast & Furious"));
    }

    #[test]
    fn appends_genre_and_runtime() {
        let showings = vec![make("Cineplexx Linz", "The Odyssey", 20, 19, 0, "OV", "Saal 6", "https://x")];
        let movies = HashMap::from([(
            "Cineplexx Linz|The Odyssey".to_string(),
            MovieMeta { runtime_min: Some(180), genres: vec!["Abenteuer".into(), "Historie".into()], poster: None },
        )]);
        let msg = format_message(&showings, &movies);
        assert!(msg.contains("<b>The Odyssey (OV)</b> — Abenteuer, Historie, 180 Min"));
    }

    #[test]
    fn meta_suffix_without_uniform_version() {
        let showings = vec![
            make("Cineplexx Linz", "F1", 20, 19, 0, "OV", "Saal 6", "https://x/1"),
            make("Cineplexx Linz", "F1", 22, 18, 30, "OmU", "Saal 1", "https://x/2"),
        ];
        let movies = HashMap::from([(
            "Cineplexx Linz|F1".to_string(),
            MovieMeta { runtime_min: Some(100), genres: vec!["Drama".into()], poster: None },
        )]);
        let msg = format_message(&showings, &movies);
        assert!(msg.contains("<b>F1</b> — Drama, 100 Min"));
    }

    #[test]
    fn escapes_meta_genres() {
        let showings = vec![make("Cineplexx Linz", "X", 20, 19, 0, "OV", "", "https://x")];
        let movies = HashMap::from([(
            "Cineplexx Linz|X".to_string(),
            MovieMeta { runtime_min: None, genres: vec!["Dra<ma> & Co".into()], poster: None },
        )]);
        let msg = format_message(&showings, &movies);
        assert!(msg.contains("Dra&lt;ma&gt; &amp; Co"));
    }

    #[test]
    fn error_format_escapes() {
        let msg = format_error("Cineplexx", &"<Response [500]> & stuff");
        assert!(msg.contains("&lt;Response [500]&gt; &amp; stuff"));
        assert!(!msg.contains("<Response [500]>"));
    }

    #[test]
    fn chunk_splits_on_line_boundaries() {
        let lines: Vec<String> = (0..200).map(|i| format!("line {i} {}", "x".repeat(90))).collect();
        let text = lines.join("\n");
        let chunks = chunk_text(&text, MAX_LEN);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.chars().count() <= MAX_LEN));
        assert_eq!(chunks.join("\n").split('\n').collect::<Vec<_>>(), lines);
    }

    #[test]
    fn chunk_hard_wraps_single_overlong_line() {
        let text = "y".repeat(5000);
        let chunks = chunk_text(&text, MAX_LEN);
        assert_eq!(chunks.len(), 2);
        assert!(chunks.iter().all(|c| c.chars().count() <= MAX_LEN));
        assert_eq!(chunks.concat(), text);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path backend/Cargo.toml notify`
Expected: FAIL to compile — functions not found. (Add `mod notify;` to `main.rs`.)

- [ ] **Step 3: Implement formatting, chunking, and the sender**

Add to `backend/src/notify.rs` (before the test module):

```rust
use std::collections::{BTreeMap, HashSet};
use std::fmt::Display;
use chrono::Datelike;

const WEEKDAYS: [&str; 7] = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];

pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn escape_attr(s: &str) -> String {
    escape_html(s).replace('"', "&quot;").replace('\'', "&#x27;")
}

pub fn format_message(showings: &[Showing], movies: &HashMap<String, MovieMeta>) -> String {
    let mut lines = vec!["🎬 <b>Neue OV-Vorstellungen in Linz</b>".to_string(), String::new()];
    let mut by_cinema: BTreeMap<&str, Vec<&Showing>> = BTreeMap::new();
    for s in showings {
        by_cinema.entry(&s.cinema).or_default().push(s);
    }
    for (cinema, group) in by_cinema {
        lines.push(format!("<b>{}</b>", escape_html(cinema)));
        let mut by_movie: HashMap<&str, Vec<&Showing>> = HashMap::new();
        for s in group {
            by_movie.entry(&s.movie).or_default().push(s);
        }
        // movie blocks ordered by their earliest showing
        let mut movies_sorted: Vec<(&str, Vec<&Showing>)> = by_movie.into_iter().collect();
        movies_sorted.sort_by_key(|(_, g)| g.iter().map(|s| s.start).min());
        for (movie, mut group) in movies_sorted {
            group.sort_by_key(|s| s.start);
            let uniform = group.iter().map(|s| &s.version).collect::<HashSet<_>>().len() == 1;
            let mut title = escape_html(movie);
            if uniform {
                title += &format!(" ({})", escape_html(&group[0].version));
            }
            let meta_suffix = movies
                .get(&format!("{cinema}|{movie}"))
                .map(|meta| {
                    let mut parts: Vec<String> =
                        meta.genres.iter().map(|g| escape_html(g)).collect();
                    if let Some(r) = meta.runtime_min {
                        parts.push(format!("{r} Min"));
                    }
                    if parts.is_empty() {
                        String::new()
                    } else {
                        format!(" — {}", parts.join(", "))
                    }
                })
                .unwrap_or_default();
            lines.push(format!("<b>{title}</b>{meta_suffix}"));
            for s in group {
                let local = s.start.with_timezone(&chrono_tz::Europe::Vienna);
                let weekday = WEEKDAYS[local.weekday().num_days_from_monday() as usize];
                let mut parts: Vec<String> = Vec::new();
                if !s.hall.is_empty() {
                    parts.push(escape_html(&s.hall));
                }
                parts.push(format!(
                    "{weekday} {}, {}",
                    local.format("%d.%m.,"),
                    local.format("%H:%M")
                ));
                if !uniform {
                    parts.push(escape_html(&s.version));
                }
                lines.push(format!(
                    "• <a href=\"{}\">{}</a>",
                    escape_attr(&s.url),
                    parts.join(" · ")
                ));
            }
        }
        lines.push(String::new());
    }
    lines.join("\n").trim().to_string()
}

pub fn format_error(source: &str, error: &dyn Display) -> String {
    format!(
        "⚠️ OV-Watcher: Quelle „{}“ scheint defekt: {}",
        escape_html(source),
        escape_html(&error.to_string())
    )
}

fn split_at_char(s: &str, n: usize) -> (String, String) {
    let byte = s.char_indices().nth(n).map(|(i, _)| i).unwrap_or(s.len());
    (s[..byte].to_string(), s[byte..].to_string())
}

/// Split text into <=limit chunks on line boundaries (hard-wrap fallback).
pub fn chunk_text(text: &str, limit: usize) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in text.split('\n') {
        let mut line = line.to_string();
        while line.chars().count() > limit {
            if !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
            let (head, tail) = split_at_char(&line, limit);
            chunks.push(head);
            line = tail;
        }
        let candidate = if current.is_empty() {
            line.clone()
        } else {
            format!("{current}\n{line}")
        };
        if candidate.chars().count() <= limit {
            current = candidate;
        } else {
            chunks.push(std::mem::take(&mut current));
            current = line;
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[async_trait::async_trait]
pub trait Notifier: Send + Sync {
    async fn send(&self, text: &str) -> anyhow::Result<()>;
}

pub struct TelegramNotifier {
    client: reqwest::Client,
    base_url: String,
    token: String,
    chat_id: String,
}

impl TelegramNotifier {
    pub fn new(token: &str, chat_id: &str) -> Self {
        Self::with_base_url(token, chat_id, "https://api.telegram.org")
    }

    pub fn with_base_url(token: &str, chat_id: &str, base_url: &str) -> Self {
        TelegramNotifier {
            client: reqwest::Client::new(),
            base_url: base_url.to_string(),
            token: token.to_string(),
            chat_id: chat_id.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl Notifier for TelegramNotifier {
    async fn send(&self, text: &str) -> anyhow::Result<()> {
        for chunk in chunk_text(text, MAX_LEN) {
            self.client
                .post(format!("{}/bot{}/sendMessage", self.base_url, self.token))
                .json(&serde_json::json!({
                    "chat_id": self.chat_id,
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

- [ ] **Step 4: Add an integration test for the HTTP payload**

Append to the `tests` module in `backend/src/notify.rs`:

```rust
    use axum::{extract::State as AxumState, routing::post, Json, Router};
    use std::sync::{Arc, Mutex};

    async fn spawn_capture_server() -> (String, Arc<Mutex<Vec<serde_json::Value>>>) {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let cap = captured.clone();
        let app = Router::new().route(
            "/botTOKEN/sendMessage",
            post(move |AxumState(()): AxumState<()>, Json(body): Json<serde_json::Value>| {
                let cap = cap.clone();
                async move {
                    cap.lock().unwrap().push(body);
                    "ok"
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}"), captured)
    }

    #[tokio::test]
    async fn send_telegram_posts_expected_payload() {
        let (base, captured) = spawn_capture_server().await;
        let notifier = TelegramNotifier::with_base_url("TOKEN", "123", &base);
        notifier.send("hello").await.unwrap();
        let calls = captured.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0],
            serde_json::json!({
                "chat_id": "123",
                "text": "hello",
                "parse_mode": "HTML",
                "link_preview_options": {"is_disabled": true},
            })
        );
    }

    #[tokio::test]
    async fn send_telegram_chunks_long_text() {
        let (base, captured) = spawn_capture_server().await;
        let notifier = TelegramNotifier::with_base_url("TOKEN", "123", &base);
        let text = "y".repeat(5000);
        notifier.send(&text).await.unwrap();
        let calls = captured.lock().unwrap();
        assert_eq!(calls.len(), 2);
        let joined: String = calls.iter().map(|c| c["text"].as_str().unwrap().to_string()).collect();
        assert_eq!(joined, text);
    }
```

- [ ] **Step 5: Run tests, then commit**

Run: `cargo test --manifest-path backend/Cargo.toml`
Expected: all passed (33 total).

```bash
git add backend/
git commit -m "Add Telegram notifier with message formatting"
```

---

### Task 7: `ics.rs` — calendar feed renderer

**Files:**
- Create: `backend/src/ics.rs`
- Modify: `backend/src/main.rs` (add `mod ics;`)

**Interfaces:**
- Consumes: `models::vienna_iso` (Task 3).
- Produces: `ics::IcsShowing { cinema: String, movie: String, start: DateTime<Utc>, version: String, hall: String, url: String, runtime_min: Option<i32> }`
- Produces: `ics::render_ics(showings: &[IcsShowing], now: DateTime<Utc>) -> String`
- Note: the Python "skip malformed showing" case disappears — `IcsShowing` is well-typed at construction (parsing happens in `web.rs`, Task 10).

- [ ] **Step 1: Write the failing tests (port of `tests/test_ics.py`)**

Create `backend/src/ics.rs`:

```rust
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct IcsShowing {
    pub cinema: String,
    pub movie: String,
    pub start: DateTime<Utc>,
    pub version: String,
    pub hall: String,
    pub url: String,
    pub runtime_min: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 31, 12, 0, 0).unwrap()
    }

    fn showing() -> IcsShowing {
        IcsShowing {
            cinema: "Cineplexx Linz".into(),
            movie: "The Odyssey".into(),
            // 2026-08-02 19:00 +02:00 == 17:00 UTC
            start: Utc.with_ymd_and_hms(2026, 8, 2, 17, 0, 0).unwrap(),
            version: "OV".into(),
            hall: "Saal 7".into(),
            url: "https://cineplexx.at/f/x".into(),
            runtime_min: None,
        }
    }

    fn render(s: &[IcsShowing]) -> String {
        render_ics(s, now())
    }

    #[test]
    fn calendar_skeleton() {
        let body = render(&[]);
        assert!(body.starts_with("BEGIN:VCALENDAR\r\n"));
        assert!(body.ends_with("END:VCALENDAR\r\n"));
        assert!(body.contains("VERSION:2.0"));
        assert!(body.contains("X-WR-CALNAME:OV Cinema Linz"));
        assert!(!body.contains("BEGIN:VEVENT"));
    }

    #[test]
    fn event_times_are_utc_and_two_hours_apart() {
        let body = render(&[showing()]);
        assert!(body.contains("DTSTART:20260802T170000Z"));
        assert!(body.contains("DTEND:20260802T190000Z"));
        assert!(body.contains("DTSTAMP:20260731T120000Z"));
    }

    #[test]
    fn summary_location_description_url() {
        let body = render(&[showing()]);
        assert!(body.contains("SUMMARY:The Odyssey (OV)"));
        assert!(body.contains("LOCATION:Cineplexx Linz\\, Saal 7"));
        assert!(body.contains("URL:https://cineplexx.at/f/x"));
        assert!(body.contains("DESCRIPTION:"));
    }

    #[test]
    fn uid_is_stable_and_matches_python_era() {
        let s = IcsShowing {
            cinema: "Megaplex PlusCity".into(),
            movie: "The Odyssey".into(),
            // 2026-08-04 19:30 +02:00 == 17:30 UTC
            start: Utc.with_ymd_and_hms(2026, 8, 4, 17, 30, 0).unwrap(),
            version: "OV".into(),
            hall: "".into(),
            url: "https://x".into(),
            runtime_min: None,
        };
        let a = render_ics(std::slice::from_ref(&s), Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
        let b = render_ics(&[s], Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap());
        let uid_a = a.split("\r\n").find(|l| l.starts_with("UID:")).unwrap();
        let uid_b = b.split("\r\n").find(|l| l.starts_with("UID:")).unwrap();
        assert_eq!(uid_a, uid_b);
        // golden value, computed from the Python implementation
        assert_eq!(
            uid_a,
            "UID:7fb86be59bcdead192c246554f3b00f5f17250c9@ov-kino-linz"
        );
    }

    #[test]
    fn text_escaping() {
        let mut s = showing();
        s.movie = "Foo, Bar; Baz".into();
        let body = render(&[s]);
        assert!(body.contains("SUMMARY:Foo\\, Bar\\; Baz (OV)"));
    }

    #[test]
    fn long_lines_folded_to_75_octets() {
        let mut s = showing();
        s.movie = "X".repeat(100);
        let body = render(&[s]);
        for line in body.split("\r\n") {
            assert!(line.len() <= 75, "line too long: {line:?}");
        }
    }

    #[test]
    fn dtend_uses_runtime_when_known() {
        let mut s = showing();
        s.runtime_min = Some(121);
        let body = render(&[s]);
        assert!(body.contains("DTEND:20260802T190100Z"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path backend/Cargo.toml ics`
Expected: FAIL to compile — `render_ics` not found. (Add `mod ics;` to `main.rs`.)

- [ ] **Step 3: Implement the renderer**

Add to `backend/src/ics.rs` (before the test module):

```rust
use sha1::{Digest, Sha1};

const CAL_HEADER: [&str; 6] = [
    "BEGIN:VCALENDAR",
    "VERSION:2.0",
    "PRODID:-//ov-kino-linz//EN",
    "CALSCALE:GREGORIAN",
    "METHOD:PUBLISH",
    "X-WR-CALNAME:OV Cinema Linz",
];
const DEFAULT_DURATION_MIN: i64 = 120;

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

/// Fold a content line to <=75-octet chunks; continuations start with a space.
fn fold(line: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut limit = 75;
    for ch in line.chars() {
        if current.len() + ch.len_utf8() > limit {
            out.push(std::mem::take(&mut current));
            current = format!(" {ch}");
            limit = 74; // leading space counts toward 75
        } else {
            current.push(ch);
        }
    }
    out.push(current);
    out
}

fn fmt_utc(dt: DateTime<Utc>) -> String {
    dt.format("%Y%m%dT%H%M%SZ").to_string()
}

fn uid(s: &IcsShowing) -> String {
    let key = format!("{}|{}|{}", s.cinema, s.movie, crate::models::vienna_iso(s.start));
    format!("{:x}@ov-kino-linz", Sha1::digest(key.as_bytes()))
}

pub fn render_ics(showings: &[IcsShowing], now: DateTime<Utc>) -> String {
    let stamp = fmt_utc(now);
    let mut lines: Vec<String> = CAL_HEADER.iter().map(|s| s.to_string()).collect();
    for s in showings {
        let duration = s.runtime_min.filter(|&r| r > 0).unwrap_or(DEFAULT_DURATION_MIN as i32) as i64;
        let end = s.start + chrono::Duration::minutes(duration);
        let summary = format!("{} ({})", s.movie, s.version);
        let location = if s.hall.is_empty() {
            s.cinema.clone()
        } else {
            format!("{}, {}", s.cinema, s.hall)
        };
        let mut description = s.version.clone();
        if !s.hall.is_empty() {
            description += &format!(", {}", s.hall);
        }
        description += &format!(" — {}", s.url);
        lines.extend([
            "BEGIN:VEVENT".to_string(),
            format!("UID:{}", uid(s)),
            format!("DTSTAMP:{stamp}"),
            format!("DTSTART:{}", fmt_utc(s.start)),
            format!("DTEND:{}", fmt_utc(end)),
            format!("SUMMARY:{}", escape(&summary)),
            format!("LOCATION:{}", escape(&location)),
            format!("DESCRIPTION:{}", escape(&description)),
            format!("URL:{}", s.url),
            "END:VEVENT".to_string(),
        ]);
    }
    lines.push("END:VCALENDAR".to_string());
    lines
        .iter()
        .flat_map(|l| fold(l))
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n"
}
```

- [ ] **Step 4: Run tests, then commit**

Run: `cargo test --manifest-path backend/Cargo.toml`
Expected: all passed (40 total).

```bash
git add backend/
git commit -m "Add ICS calendar feed renderer"
```

---

### Task 8: `checker.rs` — check orchestration + poster cache

**Files:**
- Create: `backend/src/checker.rs`
- Modify: `backend/src/main.rs` (add `mod checker;`)

**Interfaces:**
- Consumes: everything from Tasks 1–7.
- Produces: `checker::Fetcher` trait (`#[async_trait] async fn fetch(&self, http: &HttpClient, today: NaiveDate) -> Result<(Vec<Showing>, HashMap<String, MovieMeta>), SourceError>`)
- Produces: `checker::CineplexxFetcher` / `checker::MegaplexFetcher` (unit structs implementing `Fetcher`)
- Produces: `checker::CheckCtx<'a> { pool: &'a PgPool, http: &'a HttpClient, config: &'a Config, notifier: Option<&'a dyn Notifier>, fetchers: Vec<(&'a str, &'a dyn Fetcher)> }`
- Produces: `checker::CheckResult { new_showings: usize, total_showings: usize, sources: HashMap<String, String> }`
- Produces: `checker::run_check(ctx: &CheckCtx<'_>, now: DateTime<Utc>) -> anyhow::Result<CheckResult>`
- Produces: `checker::poster_filename(url: &str) -> String` (sha1(url)[:16] + whitelisted ext, default `.jpg`)

Run order in `run_check` (spec-mandated): fetch all sources → filter upcoming → poster downloads (network) → DB writes → commit → Telegram sends. This keeps network I/O out of DB transactions and Telegram sends after persistence.

- [ ] **Step 1: Write the failing tests (port of `tests/test_checker.py`)**

Create `backend/src/checker.rs`:

```rust
use crate::config::Config;
use crate::fetchers::{HttpClient, SourceError};
use crate::models::{MovieMeta, Showing};
use crate::notify::Notifier;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;
use std::collections::HashMap;

#[async_trait::async_trait]
pub trait Fetcher: Send + Sync {
    async fn fetch(
        &self,
        http: &HttpClient,
        today: NaiveDate,
    ) -> Result<(Vec<Showing>, HashMap<String, MovieMeta>), SourceError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::MovieMeta;
    use chrono::TimeZone;
    use chrono_tz::Europe::Vienna;
    use std::sync::{Arc, Mutex};

    fn now() -> DateTime<Utc> {
        Vienna.with_ymd_and_hms(2026, 7, 18, 12, 0, 0).unwrap().with_timezone(&Utc)
    }

    fn make_showing(day: u32) -> Showing {
        Showing {
            cinema: "Cineplexx Linz".into(),
            movie: "The Odyssey".into(),
            start: Vienna.with_ymd_and_hms(2026, 7, day, 19, 0, 0).unwrap().with_timezone(&Utc),
            version: "OV".into(),
            hall: "Saal 6".into(),
            url: "https://x".into(),
        }
    }

    struct FakeFetcher {
        result: Result<(Vec<Showing>, HashMap<String, MovieMeta>), String>,
    }

    #[async_trait::async_trait]
    impl Fetcher for FakeFetcher {
        async fn fetch(
            &self,
            _http: &HttpClient,
            _today: NaiveDate,
        ) -> Result<(Vec<Showing>, HashMap<String, MovieMeta>), SourceError> {
            match &self.result {
                Ok((s, m)) => Ok((s.clone(), m.clone())),
                Err(e) => Err(SourceError::msg(e.clone())),
            }
        }
    }

    #[derive(Clone, Default)]
    struct RecordingNotifier {
        sent: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl Notifier for RecordingNotifier {
        async fn send(&self, text: &str) -> anyhow::Result<()> {
            self.sent.lock().unwrap().push(text.to_string());
            Ok(())
        }
    }

    fn ctx<'a>(
        pool: &'a PgPool,
        http: &'a HttpClient,
        config: &'a Config,
        notifier: Option<&'a dyn Notifier>,
        fetcher: &'a dyn Fetcher,
    ) -> CheckCtx<'a> {
        CheckCtx { pool, http, config, notifier, fetchers: vec![("cineplexx", fetcher)] }
    }

    fn config(data_dir: &std::path::Path, telegram: bool) -> Config {
        Config {
            telegram_token: telegram.then(|| "T".to_string()),
            telegram_chat_id: telegram.then(|| "C".to_string()),
            sources: vec!["cineplexx".into()],
            check_interval: std::time::Duration::ZERO,
            data_dir: data_dir.to_path_buf(),
            port: 8080,
            database_url: String::new(),
            static_dir: std::path::PathBuf::new(),
        }
    }

    fn http() -> HttpClient {
        HttpClient::new(std::time::Duration::ZERO)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn new_showings_trigger_one_message_then_dedup(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(dir.path(), true);
        let http = http();
        let fetcher = FakeFetcher { result: Ok((vec![make_showing(20)], HashMap::new())) };
        let notifier = RecordingNotifier::default();
        let c = ctx(&pool, &http, &cfg, Some(&notifier), &fetcher);
        let r1 = run_check(&c, now()).await.unwrap();
        assert_eq!((r1.new_showings, r1.total_showings), (1, 1));
        assert_eq!(notifier.sent.lock().unwrap().len(), 1);
        assert!(notifier.sent.lock().unwrap()[0].contains("The Odyssey"));
        // second run: same showing -> no new message
        let r2 = run_check(&c, now()).await.unwrap();
        assert_eq!(r2.new_showings, 0);
        assert_eq!(notifier.sent.lock().unwrap().len(), 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn past_showings_are_dropped(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(dir.path(), true);
        let http = http();
        let fetcher = FakeFetcher { result: Ok((vec![make_showing(17)], HashMap::new())) };
        let notifier = RecordingNotifier::default();
        let c = ctx(&pool, &http, &cfg, Some(&notifier), &fetcher);
        let r = run_check(&c, now()).await.unwrap();
        assert_eq!(r.total_showings, 0);
        assert!(notifier.sent.lock().unwrap().is_empty());
        assert!(crate::db::upcoming_view(&pool, now()).await.unwrap().is_empty());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn source_error_sends_rate_limited_ping(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(dir.path(), true);
        let http = http();
        let fetcher = FakeFetcher { result: Err("kaputt".to_string()) };
        let notifier = RecordingNotifier::default();
        let c = CheckCtx { pool: &pool, http: &http, config: &cfg, notifier: Some(&notifier), fetchers: vec![("megaplex", &fetcher)] };
        let r = run_check(&c, now()).await.unwrap();
        assert_eq!(r.sources, HashMap::from([("megaplex".to_string(), "error".to_string())]));
        assert!(notifier.sent.lock().unwrap()[0].contains("kaputt"));
        // same day: no second ping
        run_check(&c, now()).await.unwrap();
        assert_eq!(notifier.sent.lock().unwrap().len(), 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn metas_filtered_to_shown_movies(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(dir.path(), false);
        let http = http();
        let metas = HashMap::from([
            ("Cineplexx Linz|The Odyssey".to_string(), MovieMeta { runtime_min: Some(180), genres: vec!["Abenteuer".into()], poster: None }),
            ("Cineplexx Linz|Not Shown".to_string(), MovieMeta { runtime_min: Some(90), genres: vec![], poster: None }),
        ]);
        let fetcher = FakeFetcher { result: Ok((vec![make_showing(20)], metas)) };
        let c = ctx(&pool, &http, &cfg, None, &fetcher);
        run_check(&c, now()).await.unwrap();
        let view = crate::db::upcoming_view(&pool, now()).await.unwrap();
        assert_eq!(view.len(), 1);
        assert_eq!(view[0].runtime_min, Some(180));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn posters_downloaded_referenced_and_pruned(pool: PgPool) {
        // local poster server serving two images
        let app = axum::Router::new()
            .route("/p.jpg", axum::routing::get(|| async { b"\xff\xd8img".as_slice() }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let poster_url = format!("http://{addr}/p.jpg");

        let dir = tempfile::tempdir().unwrap();
        let posters = dir.path().join("posters");
        std::fs::create_dir(&posters).unwrap();
        std::fs::write(posters.join("stale.jpg"), b"old").unwrap(); // unreferenced -> pruned
        let cfg = config(dir.path(), false);
        let http = http();
        let metas = HashMap::from([
            (
                "Cineplexx Linz|The Odyssey".to_string(),
                MovieMeta { runtime_min: Some(180), genres: vec![], poster: Some(poster_url.clone()) },
            ),
            // filtered out (no such showing) -> must not trigger a download
            (
                "Cineplexx Linz|Not Shown".to_string(),
                MovieMeta { runtime_min: Some(90), genres: vec![], poster: Some(format!("http://{addr}/never.jpg")) },
            ),
        ]);
        let fetcher = FakeFetcher { result: Ok((vec![make_showing(20)], metas)) };
        let c = ctx(&pool, &http, &cfg, None, &fetcher);
        run_check(&c, now()).await.unwrap();

        // exactly one downloaded file with the expected content-derived name
        let names: Vec<String> = std::fs::read_dir(&posters)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec![poster_filename(&poster_url)]);
        assert_eq!(
            std::fs::read(posters.join(poster_filename(&poster_url))).unwrap(),
            b"\xff\xd8img"
        );
        let view = crate::db::upcoming_view(&pool, now()).await.unwrap();
        assert_eq!(view[0].poster_file.as_deref(), Some(poster_filename(&poster_url).as_str()));

        // second run: file exists -> cache hit, still referenced (no download)
        run_check(&c, now()).await.unwrap();
        let view = crate::db::upcoming_view(&pool, now()).await.unwrap();
        assert_eq!(view[0].poster_file.as_deref(), Some(poster_filename(&poster_url).as_str()));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn poster_failure_is_best_effort(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(dir.path(), false);
        // no poster server running -> connection refused -> poster_file stays None
        let http = http();
        let metas = HashMap::from([(
            "Cineplexx Linz|The Odyssey".to_string(),
            MovieMeta { runtime_min: Some(180), genres: vec![], poster: Some("http://127.0.0.1:1/none.jpg".into()) },
        )]);
        let fetcher = FakeFetcher { result: Ok((vec![make_showing(20)], metas)) };
        let c = ctx(&pool, &http, &cfg, None, &fetcher);
        run_check(&c, now()).await.unwrap(); // must not raise
        let view = crate::db::upcoming_view(&pool, now()).await.unwrap();
        assert_eq!(view[0].poster_file, None);
    }

    #[test]
    fn poster_filenames() {
        // golden value from the Python implementation
        assert_eq!(
            poster_filename("https://example.com/poster.jpg"),
            "e5ef152008ef9882.jpg"
        );
        assert_eq!(poster_filename("https://x/p.webp?size=large").len(), 21);
        assert!(poster_filename("https://x/noext").ends_with(".jpg"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path backend/Cargo.toml checker`
Expected: FAIL to compile — `CheckCtx`, `run_check`, `poster_filename` not found. (Add `mod checker;` to `main.rs`.)

- [ ] **Step 3: Implement run_check + poster cache**

Add to `backend/src/checker.rs` (before the test module):

```rust
use crate::{db, notify};
use chrono_tz::Europe::Vienna;
use reqwest::header::{self, HeaderMap, HeaderValue};
use sha1::{Digest, Sha1};
use std::collections::HashSet;
use std::path::Path;

pub struct CineplexxFetcher;
pub struct MegaplexFetcher;

#[async_trait::async_trait]
impl Fetcher for CineplexxFetcher {
    async fn fetch(
        &self,
        http: &HttpClient,
        _today: NaiveDate,
    ) -> Result<(Vec<Showing>, HashMap<String, MovieMeta>), SourceError> {
        crate::fetchers::cineplexx::fetch_cineplexx(http).await
    }
}

#[async_trait::async_trait]
impl Fetcher for MegaplexFetcher {
    async fn fetch(
        &self,
        http: &HttpClient,
        today: NaiveDate,
    ) -> Result<(Vec<Showing>, HashMap<String, MovieMeta>), SourceError> {
        crate::fetchers::megaplex::fetch_megaplex(http, today).await
    }
}

pub struct CheckCtx<'a> {
    pub pool: &'a PgPool,
    pub http: &'a HttpClient,
    pub config: &'a Config,
    pub notifier: Option<&'a dyn Notifier>,
    pub fetchers: Vec<(&'a str, &'a dyn Fetcher)>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CheckResult {
    pub new_showings: usize,
    pub total_showings: usize,
    pub sources: HashMap<String, String>,
}

const POSTER_EXTS: [&str; 4] = [".jpg", ".jpeg", ".png", ".webp"];

pub fn poster_filename(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let ext = path
        .rsplit('/')
        .next()
        .and_then(|seg| seg.rsplit_once('.').map(|(_, e)| format!(".{}", e.to_lowercase())))
        .filter(|e| POSTER_EXTS.contains(&e.as_str()))
        .unwrap_or_else(|| ".jpg".to_string());
    let hash = format!("{:x}", Sha1::digest(url.as_bytes()));
    format!("{}{ext}", &hash[..16])
}

/// Download missing posters; return key -> cached basename (None if missing).
async fn cache_posters(
    http: &HttpClient,
    metas: &HashMap<String, MovieMeta>,
    posters_dir: &Path,
) -> HashMap<String, Option<String>> {
    let mut cached = HashMap::new();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::USER_AGENT,
        HeaderValue::from_static(crate::fetchers::cineplexx::USER_AGENT),
    );
    for (key, meta) in metas {
        let Some(poster) = &meta.poster else {
            cached.insert(key.clone(), None);
            continue;
        };
        let name = poster_filename(poster);
        let target = posters_dir.join(&name);
        if !target.exists() {
            if let Err(e) = tokio::fs::create_dir_all(posters_dir).await {
                tracing::warn!("poster dir: {e}");
                cached.insert(key.clone(), None);
                continue;
            }
            match http.get_bytes(poster, &headers).await {
                Ok(content) => {
                    let tmp = posters_dir.join(format!("{name}.tmp"));
                    if tokio::fs::write(&tmp, &content).await.is_ok() {
                        let _ = tokio::fs::rename(&tmp, &target).await;
                    }
                }
                Err(e) => {
                    // best-effort: retry on the next run
                    tracing::warn!("poster download failed for {key}: {e}");
                    cached.insert(key.clone(), None);
                    continue;
                }
            }
        }
        cached.insert(key.clone(), Some(name));
    }
    cached
}

async fn prune_posters(posters_dir: &Path, keep: &HashSet<String>) {
    let Ok(mut entries) = tokio::fs::read_dir(posters_dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if entry.path().is_file() && !keep.contains(&name) {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }
}

pub async fn run_check(ctx: &CheckCtx<'_>, now: DateTime<Utc>) -> anyhow::Result<CheckResult> {
    let today = now.with_timezone(&Vienna).date_naive();
    let mut all_showings: Vec<Showing> = Vec::new();
    let mut all_metas: HashMap<String, MovieMeta> = HashMap::new();
    let mut health: HashMap<String, String> = HashMap::new();
    let mut pending_sends: Vec<String> = Vec::new();

    // 1. fetch all sources (network)
    for (source, fetcher) in &ctx.fetchers {
        match fetcher.fetch(ctx.http, today).await {
            Ok((showings, metas)) => {
                all_showings.extend(showings);
                all_metas.extend(metas);
                health.insert(source.to_string(), "ok".to_string());
            }
            Err(e) => {
                health.insert(source.to_string(), "error".to_string());
                let already = db::get_source_status(ctx.pool, source)
                    .await?
                    .and_then(|(_, d)| d)
                    == Some(today);
                if ctx.notifier.is_some() && !already {
                    pending_sends.push(notify::format_error(source, &e));
                }
                db::upsert_source_status(ctx.pool, source, "error", Some(today)).await?;
            }
        }
    }

    // 2. keep only upcoming
    let upcoming: Vec<Showing> = all_showings.into_iter().filter(|s| s.start >= now).collect();
    let wanted: HashSet<String> =
        upcoming.iter().map(|s| format!("{}|{}", s.cinema, s.movie)).collect();
    let filtered_metas: HashMap<String, MovieMeta> = all_metas
        .into_iter()
        .filter(|(k, _)| wanted.contains(k))
        .collect();

    // 3. poster downloads (network, before DB writes)
    let posters_dir = ctx.config.data_dir.join("posters");
    let poster_files = cache_posters(ctx.http, &filtered_metas, &posters_dir).await;
    prune_posters(
        &posters_dir,
        &poster_files.values().flatten().cloned().collect(),
    )
    .await;

    // 4. DB writes
    let mut new_showings: Vec<Showing> = Vec::new();
    for s in &upcoming {
        let key = format!("{}|{}", s.cinema, s.movie);
        let meta = filtered_metas.get(&key);
        let poster_file = poster_files.get(&key).and_then(|f| f.as_deref());
        let movie_id = db::upsert_movie(
            ctx.pool,
            &s.cinema,
            &s.movie,
            meta.and_then(|m| m.runtime_min),
            meta.map(|m| m.genres.as_slice()).unwrap_or(&[]),
            meta.and_then(|m| m.poster.as_deref()),
            poster_file,
        )
        .await?;
        if db::insert_showing(ctx.pool, movie_id, s.start, &s.version, &s.hall, &s.url, now).await? {
            new_showings.push(s.clone());
        }
    }
    db::prune(ctx.pool, now - chrono::Duration::hours(6)).await?;
    db::insert_check_run(ctx.pool, now, new_showings.len() as i32, upcoming.len() as i32).await?;
    for source in health.keys() {
        if health[source] == "ok" {
            db::upsert_source_status(ctx.pool, source, "ok", None).await?;
        }
    }

    // 5. Telegram sends (after persistence; error pings first, then the
    // new-showings message — same order as the Python checker)
    if !new_showings.is_empty() && ctx.notifier.is_some() {
        pending_sends.push(notify::format_message(&new_showings, &filtered_metas));
    }
    if let Some(notifier) = ctx.notifier {
        for text in &pending_sends {
            notifier.send(text).await?;
        }
    }

    Ok(CheckResult {
        new_showings: new_showings.len(),
        total_showings: upcoming.len(),
        sources: health,
    })
}
```

- [ ] **Step 4: Run tests, then commit**

Run: `cargo test --manifest-path backend/Cargo.toml`
Expected: all passed (46 total).

```bash
git add backend/
git commit -m "Add check orchestration with poster cache"
```

---

### Task 9: Scheduler + main wiring

**Files:**
- Modify: `backend/src/main.rs` (full rewrite)

**Interfaces:**
- Consumes: everything from Tasks 1–8.
- Produces: `main::scheduler_loop(interval: Duration, run: impl Fn() -> Fut) -> !`-style loop where `Fut: Future<Output = anyhow::Result<()>>` — first run immediate (tokio `interval` fires instantly, parity with the Python scheduler)
- Produces: `main::run_default_check(pool: &PgPool, config: &Config) -> anyhow::Result<()>`

- [ ] **Step 1: Write the failing scheduler test**

Replace `backend/src/main.rs` with (test module only; the rest comes in Step 3):

```rust
mod checker;
mod config;
mod db;
mod fetchers;
mod ics;
mod models;
mod notify;
mod web;

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test(start_paused = true)]
    async fn scheduler_runs_immediately_and_repeatedly() {
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let handle = tokio::spawn(super::scheduler_loop(Duration::from_secs(3600), move || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }));
        tokio::time::advance(Duration::from_secs(7200)).await;
        for _ in 0..10 {
            tokio::task::yield_now().await; // let the spawned loop consume its ticks
        }
        handle.abort();
        assert_eq!(count.load(Ordering::SeqCst), 3); // immediate + 2 intervals
    }

    #[tokio::test(start_paused = true)]
    async fn scheduler_survives_failures() {
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let handle = tokio::spawn(super::scheduler_loop(Duration::from_secs(60), move || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err(anyhow::anyhow!("boom"))
            }
        }));
        tokio::time::advance(Duration::from_secs(120)).await;
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        handle.abort();
        assert_eq!(count.load(Ordering::SeqCst), 3);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path backend/Cargo.toml scheduler`
Expected: FAIL to compile — `scheduler_loop` not found.

- [ ] **Step 3: Implement scheduler_loop, run_default_check, and main**

Replace `backend/src/main.rs` module-level code (keep the test module, add `mod import;` only in Task 13):

```rust
mod checker;
mod config;
mod db;
mod fetchers;
mod ics;
mod models;
mod notify;
mod web;

use checker::{CineplexxFetcher, Fetcher, MegaplexFetcher};
use chrono::Utc;
use config::Config;
use fetchers::HttpClient;
use notify::{Notifier, TelegramNotifier};
use sqlx::PgPool;
use std::future::Future;
use std::net::SocketAddr;
use std::time::Duration;

pub async fn scheduler_loop<F, Fut>(interval: Duration, run: F)
where
    F: Fn() -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await; // first tick fires immediately
        if let Err(e) = run().await {
            tracing::error!("check run failed: {e:#}");
        }
    }
}

async fn run_default_check(pool: &PgPool, config: &Config) -> anyhow::Result<()> {
    let http = HttpClient::new(Duration::from_millis(500));
    let cineplexx = CineplexxFetcher;
    let megaplex = MegaplexFetcher;
    let mut active: Vec<(&str, &dyn Fetcher)> = Vec::new();
    for source in &config.sources {
        match source.as_str() {
            "cineplexx" => active.push(("cineplexx", &cineplexx)),
            "megaplex" => active.push(("megaplex", &megaplex)),
            other => tracing::warn!("unknown source {other}"),
        }
    }
    let notifier = match (&config.telegram_token, &config.telegram_chat_id) {
        (Some(token), Some(chat_id)) => Some(TelegramNotifier::new(token, chat_id)),
        _ => None,
    };
    let ctx = checker::CheckCtx {
        pool,
        http: &http,
        config,
        notifier: notifier.as_ref().map(|n| n as &dyn Notifier),
        fetchers: active,
    };
    let result = checker::run_check(&ctx, Utc::now()).await?;
    tracing::info!(new = result.new_showings, total = result.total_showings, "check finished");
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ov_watcher=info,tower_http=info".into()),
        )
        .init();
    let config = Config::from_env()?;
    let pool = PgPool::connect(&config.database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    {
        let pool = pool.clone();
        let config = config.clone();
        tokio::spawn(async move {
            scheduler_loop(config.check_interval, move || {
                let pool = pool.clone();
                let config = config.clone();
                async move { run_default_check(&pool, &config).await }
            })
            .await;
        });
    }

    let state = web::AppState {
        pool,
        data_dir: config.data_dir.clone(),
        static_dir: config.static_dir.clone(),
    };
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!("starting web server on port {}", config.port);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, web::router(state)).await?;
    Ok(())
}
```

(The `web::router`/`AppState` don't exist yet — Task 10 adds them. For this task's tests to compile, temporarily keep `web.rs` as-is and comment the serve block, or implement Task 9 and Task 10's Step 1–3 together before running `cargo test` on the full suite. Recommended order: implement Step 3 with the serve block **excluded**, run scheduler tests, then Task 10 adds the serve block back.)

- [ ] **Step 4: Run tests, then commit**

Run: `cargo test --manifest-path backend/Cargo.toml scheduler`
Expected: 2 passed.

```bash
git add backend/
git commit -m "Add scheduler loop and main wiring"
```

---

### Task 10: Web API — `/api/showings`, `/showings.ics`, `/posters`, SPA statics

**Files:**
- Modify: `backend/src/web.rs` (full rewrite)

**Interfaces:**
- Consumes: `db::{ShowingView, upcoming_view, latest_check_run, all_source_statuses}` (Task 2), `ics::{IcsShowing, render_ics}` (Task 7).
- Produces: `web::AppState { pool: PgPool, data_dir: PathBuf, static_dir: PathBuf }`
- Produces: `web::router(state: AppState) -> Router`
- Produces: `web::ApiPayload { generated_at: Option<String>, sources: Option<HashMap<String,String>>, cinemas: Option<Vec<CinemaView>> }` serialized camelCase (`generatedAt`, `metaLine`, ...)
- Produces: `web::build_payload(run_at: DateTime<Utc>, statuses: Vec<(String, String)>, views: Vec<ShowingView>) -> ApiPayload` (pure — port of `_group_showings` from `app/web.py`)

Behavior contract (port of `app/web.py`):
- Cinemas ordered: `Megaplex PlusCity` first, then alphabetical. Movies ordered by earliest showing (insertion order from the start-sorted query gives this for free).
- Badge: if all of a movie's showings share the same base version (`version.split(" - ")[0].trim()`), it's the badge; mixed bases → `null`.
- Detail per showing: badge present → `_short_version(version)` (strip leading `OV`/`OV - `) + hall, joined `, `; badge absent → full/shortened version + hall. Empty → `""`.
- `metaLine`: genres joined `, ` + `N Min`, joined ` · ` (empty when neither).
- `date` = `"Mon 04.08."` (English weekday abbreviations), `time` = `"19:30"`, both in Europe/Vienna.
- `generatedAt`: `run_at` in Europe/Vienna formatted `%Y-%m-%d %H:%M`.
- `/posters/{name}`: 404 unless `name` is a safe single filename (`[A-Za-z0-9._-]+`, not starting with `.`); `Cache-Control: max-age=86400`; content type by extension (`.png` → `image/png`, `.webp` → `image/webp`, else `image/jpeg`).
- `/showings.ics`: `text/calendar; charset=utf-8`; on DB error → empty calendar (best-effort, parity with missing payload).
- SPA: all other GET paths serve `static_dir`, falling back to `static_dir/index.html`.

- [ ] **Step 1: Write the failing tests**

Replace `backend/src/web.rs` with:

```rust
use crate::db::ShowingView;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub data_dir: PathBuf,
    pub static_dir: PathBuf,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiPayload {
    pub generated_at: Option<String>,
    pub sources: Option<HashMap<String, String>>,
    pub cinemas: Option<Vec<CinemaView>>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CinemaView {
    pub name: String,
    pub movies: Vec<MovieView>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MovieView {
    pub title: String,
    pub badge: Option<String>,
    pub meta_line: String,
    pub poster: Option<String>,
    pub showings: Vec<ShowingRow>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShowingRow {
    pub date: String,
    pub time: String,
    pub detail: String,
    pub url: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use chrono_tz::Europe::Vienna;

    fn view(cinema: &str, movie: &str, day: u32, hour: u32, version: &str, hall: &str) -> ShowingView {
        ShowingView {
            cinema: cinema.into(),
            movie: movie.into(),
            start: Vienna.with_ymd_and_hms(2026, 8, day, hour, 30, 0).unwrap().with_timezone(&Utc),
            version: version.into(),
            hall: hall.into(),
            url: "https://x".into(),
            runtime_min: None,
            genres: vec![],
            poster_file: None,
        }
    }

    fn run_at() -> DateTime<Utc> {
        Vienna.with_ymd_and_hms(2026, 8, 2, 12, 0, 0).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn payload_none_states() {
        // tested at the handler level; build_payload always has data
        let p = ApiPayload { generated_at: None, sources: None, cinemas: None };
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json, serde_json::json!({"generatedAt": null, "sources": null, "cinemas": null}));
    }

    #[test]
    fn groups_showings_and_formats_rows() {
        let mut odyssey = view("Cineplexx Linz", "The Odyssey", 4, 19, "OV", "Saal 7");
        odyssey.runtime_min = Some(121);
        odyssey.genres = vec!["Abenteuer".into(), "Historie".into()];
        odyssey.poster_file = Some("a.jpg".into());
        let views = vec![
            odyssey,
            view("Megaplex PlusCity", "Die Odyssee", 3, 20, "OV - IMAX 2D", ""),
        ];
        let p = build_payload(run_at(), vec![("cineplexx".into(), "ok".into())], views);
        assert_eq!(p.generated_at.as_deref(), Some("2026-08-02 12:00"));
        // Megaplex first despite later in the alphabet
        assert_eq!(p.cinemas.as_ref().unwrap()[0].name, "Megaplex PlusCity");
        let cineplexx = &p.cinemas.as_ref().unwrap()[1];
        let m = &cineplexx.movies[0];
        assert_eq!(m.title, "The Odyssey");
        assert_eq!(m.badge.as_deref(), Some("OV"));
        assert_eq!(m.meta_line, "Abenteuer, Historie · 121 Min");
        assert_eq!(m.poster.as_deref(), Some("a.jpg"));
        assert_eq!(m.showings[0].date, "Tue 04.08.");
        assert_eq!(m.showings[0].time, "19:30");
        assert_eq!(m.showings[0].detail, "Saal 7"); // badge=OV -> short version "" + hall
        let mega = &p.cinemas.as_ref().unwrap()[0].movies[0];
        assert_eq!(mega.badge.as_deref(), Some("OV"));
        assert_eq!(mega.showings[0].detail, "IMAX 2D"); // "OV - IMAX 2D" -> "IMAX 2D"
    }

    #[test]
    fn mixed_versions_drop_the_badge() {
        let views = vec![
            view("Cineplexx Linz", "F1", 4, 19, "OV", "Saal 6"),
            view("Cineplexx Linz", "F1", 5, 18, "OmU", "Saal 1"),
        ];
        let p = build_payload(run_at(), vec![], views);
        let m = &p.cinemas.as_ref().unwrap()[0].movies[0];
        assert_eq!(m.badge, None);
        assert_eq!(m.showings[0].detail, "OV, Saal 6");
        assert_eq!(m.showings[1].detail, "OmU, Saal 1");
    }

    #[test]
    fn same_day_mixed_variants_keep_shared_base_badge() {
        let views = vec![
            view("Megaplex PlusCity", "Die Odyssee", 3, 19, "OV - IMAX 2D", ""),
            view("Megaplex PlusCity", "Die Odyssee", 4, 20, "OV - Dolby Vision 2D", ""),
        ];
        let p = build_payload(run_at(), vec![], views);
        let m = &p.cinemas.as_ref().unwrap()[0].movies[0];
        assert_eq!(m.badge.as_deref(), Some("OV"));
        assert_eq!(m.showings[0].detail, "IMAX 2D");
        assert_eq!(m.showings[1].detail, "Dolby Vision 2D");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn api_showings_three_states(pool: PgPool) {
        use axum::body::to_bytes;
        use axum::http::Request;
        use tower::ServiceExt;

        let state = AppState { pool: pool.clone(), data_dir: PathBuf::new(), static_dir: PathBuf::from("/nonexistent") };
        let app = router(state);
        // state 1: no check run yet -> nulls
        let resp = app.clone().oneshot(Request::get("/api/showings").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["cinemas"], serde_json::Value::Null);
        // state 2: check run, no showings -> []
        crate::db::insert_check_run(&pool, Utc::now(), 0, 0).await.unwrap();
        let resp = app.clone().oneshot(Request::get("/api/showings").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["cinemas"], serde_json::json!([]));
        // state 3: data present
        let mid = crate::db::upsert_movie(&pool, "Cineplexx Linz", "F1", None, &[], None, None).await.unwrap();
        crate::db::insert_showing(&pool, mid, Utc::now() + chrono::Duration::days(1), "OV", "Saal 6", "https://x", Utc::now()).await.unwrap();
        let resp = app.oneshot(Request::get("/api/showings").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["cinemas"][0]["movies"][0]["title"], "F1");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn ics_route_renders_events(pool: PgPool) {
        use axum::body::to_bytes;
        use axum::http::Request;
        use tower::ServiceExt;

        let mid = crate::db::upsert_movie(&pool, "Cineplexx Linz", "F1", Some(121), &[], None, None).await.unwrap();
        crate::db::insert_showing(&pool, mid, Utc::now() + chrono::Duration::days(1), "OV", "Saal 6", "https://x", Utc::now()).await.unwrap();
        let state = AppState { pool, data_dir: PathBuf::new(), static_dir: PathBuf::from("/nonexistent") };
        let resp = router(state).oneshot(Request::get("/showings.ics").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(
            resp.headers()["content-type"].to_str().unwrap(),
            "text/calendar; charset=utf-8"
        );
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert_eq!(text.matches("BEGIN:VEVENT").count(), 1);
        assert!(text.contains("SUMMARY:F1 (OV)"));
    }

    #[tokio::test]
    async fn poster_route_serves_and_guards() {
        use axum::body::to_bytes;
        use axum::http::Request;
        use tower::ServiceExt;

        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("posters")).unwrap();
        std::fs::write(dir.path().join("posters/a1b2.jpg"), b"img").unwrap();
        // a pool is required for AppState but unused by this route; lazy-connect
        let pool = PgPool::connect_lazy("postgres://ov:ov@localhost/ov").unwrap();
        let state = AppState { pool, data_dir: dir.path().to_path_buf(), static_dir: PathBuf::from("/nonexistent") };
        let app = router(state);
        let resp = app.clone().oneshot(Request::get("/posters/a1b2.jpg").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.headers()["cache-control"].to_str().unwrap(), "max-age=86400");
        assert_eq!(resp.headers()["content-type"].to_str().unwrap(), "image/jpeg");
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"img");
        // traversal / dotfile attempts are rejected
        for bad in ["..", ".hidden", "..%2Fetc"] {
            let resp = app.clone().oneshot(Request::get(format!("/posters/{bad}")).body(axum::body::Body::empty()).unwrap()).await.unwrap();
            assert_eq!(resp.status(), 404, "expected 404 for {bad}");
        }
        let resp = app.oneshot(Request::get("/posters/missing.jpg").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(resp.status(), 404);
    }

    #[tokio::test]
    async fn healthz_route() {
        use axum::body::to_bytes;
        use axum::http::Request;
        use tower::ServiceExt;

        let pool = PgPool::connect_lazy("postgres://ov:ov@localhost/ov").unwrap();
        let state = AppState { pool, data_dir: PathBuf::new(), static_dir: PathBuf::from("/nonexistent") };
        let resp = router(state).oneshot(Request::get("/healthz").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(resp.status(), 200);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"ok");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path backend/Cargo.toml web`
Expected: FAIL to compile — `build_payload`, `router` not found.

- [ ] **Step 3: Implement the routes and view-model assembly**

Add to `backend/src/web.rs` (before the test module):

```rust
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use axum::Router;
use chrono::Datelike;
use chrono_tz::Europe::Vienna;
use std::collections::HashSet;
use tower_http::services::{ServeDir, ServeFile};

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const CINEMA_ORDER: [&str; 1] = ["Megaplex PlusCity"];

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/showings", get(api_showings))
        .route("/showings.ics", get(showings_ics))
        .route("/posters/{name}", get(poster))
        .route("/healthz", get(healthz))
        .fallback_service(
            ServeDir::new(&state.static_dir)
                .not_found_service(ServeFile::new(state.static_dir.join("index.html"))),
        )
        .with_state(state)
}

pub async fn healthz() -> &'static str {
    "ok"
}

async fn api_showings(State(state): State<AppState>) -> Result<Json<ApiPayload>, StatusCode> {
    let run_at = crate::db::latest_check_run(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match run_at {
        None => Ok(Json(ApiPayload { generated_at: None, sources: None, cinemas: None })),
        Some(run_at) => {
            let views = crate::db::upcoming_view(&state.pool, Utc::now())
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let statuses = crate::db::all_source_statuses(&state.pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(Json(build_payload(run_at, statuses, views)))
        }
    }
}

async fn showings_ics(State(state): State<AppState>) -> Response {
    let views = crate::db::upcoming_view(&state.pool, Utc::now())
        .await
        .unwrap_or_default();
    let showings: Vec<crate::ics::IcsShowing> = views
        .into_iter()
        .map(|v| crate::ics::IcsShowing {
            cinema: v.cinema,
            movie: v.movie,
            start: v.start,
            version: v.version,
            hall: v.hall,
            url: v.url,
            runtime_min: v.runtime_min,
        })
        .collect();
    let body = crate::ics::render_ics(&showings, Utc::now());
    ([(header::CONTENT_TYPE, "text/calendar; charset=utf-8")], body).into_response()
}

async fn poster(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    let safe = !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if !safe {
        return StatusCode::NOT_FOUND.into_response();
    }
    let path = state.data_dir.join("posters").join(&name);
    let Ok(bytes) = tokio::fs::read(&path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = match path.extension().and_then(|e| e.to_str()) {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    };
    (
        [(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "max-age=86400")],
        bytes,
    )
        .into_response()
}

fn short_version(version: &str) -> &str {
    let v = version.trim();
    if v == "OV" {
        ""
    } else if let Some(rest) = v.strip_prefix("OV - ") {
        rest.trim()
    } else {
        v
    }
}

pub fn build_payload(
    run_at: DateTime<Utc>,
    statuses: Vec<(String, String)>,
    views: Vec<ShowingView>,
) -> ApiPayload {
    // group by cinema, then movie, preserving order of first appearance
    // (the query is already sorted by start, cinema)
    let mut cinemas: Vec<(String, Vec<(String, Vec<&ShowingView>)>)> = Vec::new();
    for v in &views {
        let cinema_group = match cinemas.iter_mut().find(|(name, _)| name == &v.cinema) {
            Some((_, movies)) => movies,
            None => {
                cinemas.push((v.cinema.clone(), Vec::new()));
                &mut cinemas.last_mut().unwrap().1
            }
        };
        match cinema_group.iter_mut().find(|(title, _)| title == &v.movie) {
            Some((_, group)) => group.push(v),
            None => cinema_group.push((v.movie.clone(), vec![v])),
        }
    }
    cinemas.sort_by_key(|(name, _)| {
        (
            CINEMA_ORDER.iter().position(|c| c == name).unwrap_or(CINEMA_ORDER.len()),
            name.clone(),
        )
    });
    let cinemas = cinemas
        .into_iter()
        .map(|(name, movies)| CinemaView {
            name,
            movies: movies
                .into_iter()
                .map(|(title, group)| movie_view(title, &group))
                .collect(),
        })
        .collect();
    ApiPayload {
        generated_at: Some(
            run_at.with_timezone(&Vienna).format("%Y-%m-%d %H:%M").to_string(),
        ),
        sources: Some(statuses.into_iter().collect()),
        cinemas: Some(cinemas),
    }
}

fn movie_view(title: String, group: &[&ShowingView]) -> MovieView {
    let bases: HashSet<&str> = group
        .iter()
        .map(|s| s.version.split(" - ").next().unwrap_or("").trim())
        .collect();
    let badge = if bases.len() == 1 {
        bases.into_iter().next().map(str::to_string)
    } else {
        None
    };
    let first = group[0];
    let mut meta_parts: Vec<String> = first.genres.clone();
    if let Some(r) = first.runtime_min {
        meta_parts.push(format!("{r} Min"));
    }
    let showings = group
        .iter()
        .map(|s| {
            let local = s.start.with_timezone(&Vienna);
            let mut parts: Vec<String> = Vec::new();
            let variant = short_version(&s.version);
            match &badge {
                None => parts.push(if variant.is_empty() { s.version.clone() } else { variant.to_string() }),
                Some(_) if !variant.is_empty() => parts.push(variant.to_string()),
                _ => {}
            }
            if !s.hall.is_empty() {
                parts.push(s.hall.clone());
            }
            ShowingRow {
                date: format!(
                    "{} {}",
                    WEEKDAYS[local.weekday().num_days_from_monday() as usize],
                    local.format("%d.%m.")
                ),
                time: local.format("%H:%M").to_string(),
                detail: parts.join(", "),
                url: s.url.clone(),
            }
        })
        .collect();
    MovieView {
        title,
        badge,
        meta_line: meta_parts.join(" · "),
        poster: first.poster_file.clone(),
        showings,
    }
}
```

- [ ] **Step 4: Re-enable the serve block in main.rs**

In `backend/src/main.rs`, ensure the web-serving block from Task 9 Step 3 (constructing `web::AppState`, binding, `axum::serve`) is active.

- [ ] **Step 5: Run tests, then commit**

Run: `cargo test --manifest-path backend/Cargo.toml`
Expected: all passed (54 total).

```bash
git add backend/
git commit -m "Add web API, ICS route, poster serving and SPA statics"
```

---

### Task 11: Frontend scaffold — Vite + React + TS, types, API client

**Files:**
- Create: `frontend/package.json`, `frontend/tsconfig.json`, `frontend/vite.config.ts`, `frontend/index.html`, `frontend/.gitignore`
- Create: `frontend/src/main.tsx`, `frontend/src/types.ts`, `frontend/src/api.ts`, `frontend/src/api.test.ts`, `frontend/src/test/setup.ts`

**Interfaces:**
- Produces: `types.ApiPayload` / `types.CinemaView` / `types.MovieView` / `types.ShowingRow` (camelCase, matching Task 10's JSON)
- Produces: `api.fetchShowings(): Promise<ApiPayload>` (throws on non-2xx)

- [ ] **Step 1: Scaffold files**

Create `frontend/package.json`:

```json
{
  "name": "ov-cinema-frontend",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc --noEmit && vite build",
    "test": "vitest run"
  },
  "dependencies": {
    "react": "^19.1.0",
    "react-dom": "^19.1.0"
  },
  "devDependencies": {
    "@testing-library/jest-dom": "^6.6.3",
    "@testing-library/react": "^16.3.0",
    "@types/react": "^19.1.0",
    "@types/react-dom": "^19.1.0",
    "@vitejs/plugin-react": "^4.4.0",
    "jsdom": "^26.1.0",
    "typescript": "^5.8.0",
    "vite": "^6.3.0",
    "vitest": "^3.1.0"
  }
}
```

Create `frontend/tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "skipLibCheck": true,
    "isolatedModules": true,
    "noEmit": true,
    "types": ["vitest/globals", "@testing-library/jest-dom"]
  },
  "include": ["src", "vite.config.ts"]
}
```

Create `frontend/vite.config.ts`:

```ts
/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      "/api": "http://localhost:8080",
      "/posters": "http://localhost:8080",
      "/showings.ics": "http://localhost:8080",
      "/healthz": "http://localhost:8080",
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: "./src/test/setup.ts",
  },
});
```

Create `frontend/index.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>OV Cinema Linz</title>
    <link rel="icon" type="image/svg+xml" href="/favicon.svg" />
    <link rel="preconnect" href="https://fonts.googleapis.com" />
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin />
    <link
      href="https://fonts.googleapis.com/css2?family=Limelight&display=swap"
      rel="stylesheet"
    />
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

Create `frontend/.gitignore`:

```
/node_modules
/dist
```

Copy the favicon: `mkdir -p frontend/public && cp app/static/favicon.svg frontend/public/favicon.svg`

Create `frontend/src/test/setup.ts`:

```ts
import "@testing-library/jest-dom/vitest";
```

Create `frontend/src/types.ts`:

```ts
export interface ShowingRow {
  date: string;
  time: string;
  detail: string;
  url: string;
}

export interface MovieView {
  title: string;
  badge: string | null;
  metaLine: string;
  poster: string | null;
  showings: ShowingRow[];
}

export interface CinemaView {
  name: string;
  movies: MovieView[];
}

export interface ApiPayload {
  generatedAt: string | null;
  sources: Record<string, string> | null;
  cinemas: CinemaView[] | null;
}
```

- [ ] **Step 2: Write the failing API test**

Create `frontend/src/api.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { fetchShowings } from "./api";

afterEach(() => vi.unstubAllGlobals());

describe("fetchShowings", () => {
  it("returns the parsed payload", async () => {
    const payload = { generatedAt: "2026-08-02 12:00", sources: { cineplexx: "ok" }, cinemas: [] };
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({ ok: true, json: async () => payload }))
    );
    await expect(fetchShowings()).resolves.toEqual(payload);
    expect(fetch).toHaveBeenCalledWith("/api/showings");
  });

  it("throws on http errors", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => ({ ok: false, status: 500 })));
    await expect(fetchShowings()).rejects.toThrow("500");
  });
});
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cd frontend && npm install && npx vitest run src/api.test.ts`
Expected: FAIL — `./api` module not found.

- [ ] **Step 4: Implement the API client and entrypoint**

Create `frontend/src/api.ts`:

```ts
import type { ApiPayload } from "./types";

export async function fetchShowings(): Promise<ApiPayload> {
  const resp = await fetch("/api/showings");
  if (!resp.ok) {
    throw new Error(`GET /api/showings failed: ${resp.status}`);
  }
  return (await resp.json()) as ApiPayload;
}
```

Create `frontend/src/main.tsx`:

```tsx
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>
);
```

(`src/App.tsx` and `src/index.css` are added in Task 12; create empty placeholders now so the build passes: `touch frontend/src/App.tsx frontend/src/index.css` — App gets its real content next task.)

- [ ] **Step 5: Run test, then commit**

Run: `cd frontend && npx vitest run`
Expected: 2 passed.

```bash
git add frontend/
git commit -m "Add frontend scaffold with typed API client"
```

---

### Task 12: Frontend components + CSS port (visual 1:1)

**Files:**
- Create: `frontend/src/App.tsx`, `frontend/src/App.test.tsx`
- Create: `frontend/src/components/Marquee.tsx`, `frontend/src/components/Sidebar.tsx`, `frontend/src/components/CinemaSection.tsx`, `frontend/src/components/MovieCard.tsx`, `frontend/src/components/MovieCard.test.tsx`
- Create: `frontend/src/index.css`

**Interfaces:**
- Consumes: `types.ApiPayload` etc. and `api.fetchShowings` (Task 11).

- [ ] **Step 1: Write the failing component tests**

Create `frontend/src/components/MovieCard.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MovieCard } from "./MovieCard";
import type { MovieView } from "../types";

const movie: MovieView = {
  title: "The Odyssey",
  badge: "OV",
  metaLine: "Abenteuer, Historie · 180 Min",
  poster: "a1b2.jpg",
  showings: [
    { date: "Mo 04.08.", time: "19:30", detail: "Saal 7", url: "https://x/1" },
    { date: "Di 05.08.", time: "20:15", detail: "", url: "https://x/2" },
  ],
};

describe("MovieCard", () => {
  it("renders title, badge, meta line and poster", () => {
    render(<MovieCard movie={movie} />);
    expect(screen.getByText("The Odyssey")).toBeInTheDocument();
    expect(screen.getByText("OV")).toHaveClass("badge");
    expect(screen.getByText("Abenteuer, Historie · 180 Min")).toHaveClass("filmmeta");
    expect(screen.getByRole("img")).toHaveAttribute("src", "/posters/a1b2.jpg");
  });

  it("renders one link per showing, omitting empty details", () => {
    render(<MovieCard movie={movie} />);
    const links = screen.getAllByRole("link");
    expect(links).toHaveLength(2);
    expect(links[0]).toHaveAttribute("href", "https://x/1");
    expect(links[0]).toHaveTextContent("Mo 04.08. · 19:30");
    expect(links[0]).toHaveTextContent("Saal 7");
    expect(links[1].querySelector(".detail")).toBeNull();
  });

  it("omits badge, meta and poster when absent", () => {
    render(
      <MovieCard movie={{ title: "F1", badge: null, metaLine: "", poster: null, showings: [] }} />
    );
    expect(screen.queryByRole("img")).toBeNull();
    expect(screen.getByText("F1")).toBeInTheDocument();
  });
});
```

Create `frontend/src/App.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "./App";

const payload = {
  generatedAt: "2026-08-02 12:00",
  sources: { cineplexx: "ok", megaplex: "error" },
  cinemas: [
    {
      name: "Megaplex PlusCity",
      movies: [
        {
          title: "Die Odyssee",
          badge: "OV",
          metaLine: "Drama · 173 Min",
          poster: null,
          showings: [{ date: "Mo 04.08.", time: "19:30", detail: "IMAX 2D", url: "https://x" }],
        },
      ],
    },
  ],
};

function mockFetch(body: unknown) {
  vi.stubGlobal("fetch", vi.fn(async () => ({ ok: true, json: async () => body })));
}

afterEach(() => vi.unstubAllGlobals());

describe("App", () => {
  it("shows 'first check' state when cinemas is null", async () => {
    mockFetch({ generatedAt: null, sources: null, cinemas: null });
    render(<App />);
    expect(await screen.findByText(/first check is running/)).toBeInTheDocument();
  });

  it("shows the empty state", async () => {
    mockFetch({ generatedAt: "x", sources: {}, cinemas: [] });
    render(<App />);
    expect(await screen.findByText(/No OV showings found/)).toBeInTheDocument();
  });

  it("renders cinemas, footer and source health", async () => {
    mockFetch(payload);
    render(<App />);
    expect(await screen.findByText("Megaplex PlusCity")).toBeInTheDocument();
    expect(screen.getByText("Die Odyssee")).toBeInTheDocument();
    expect(screen.getByText("error")).toHaveClass("err");
    expect(screen.getByText("ok")).toHaveClass("ok");
    expect(screen.getByText(/Last checked: 2026-08-02 12:00/)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && npx vitest run`
Expected: FAIL — components not found.

- [ ] **Step 3: Implement components, App and CSS**

Create `frontend/src/components/Marquee.tsx`:

```tsx
export function Marquee() {
  return (
    <header className="marquee">
      <div className="bulbs"></div>
      <h1>🎬 OV Cinema Linz</h1>
      <p className="tagline">Original Versions in Linz</p>
      <div className="bulbs"></div>
    </header>
  );
}
```

Create `frontend/src/components/Sidebar.tsx`:

```tsx
export function Sidebar() {
  return (
    <aside className="sidebar">
      <div className="box">
        <span className="icon tg">
          <svg viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
            <circle cx="24" cy="24" r="24" fill="#229ED9" />
            <path
              fill="#fff"
              d="M10.7 23.5l25-9.6c1.2-.4 2.2.3 1.8 2l-4.3 20c-.3 1.3-1 1.6-2 1l-6-4.4-2.9 2.8c-.3.3-.6.6-1.2.6l.4-6 10.6-9.6c.5-.4-.1-.6-.7-.2L17.2 22l-5.9-1.8c-1.3-.4-1.3-1.3.3-2z"
            />
          </svg>
        </span>
        <span className="text">
          Get notified about new OV showings on Telegram
          <span className="sub">Channel: @ov_linz — free, no spam, only new showings.</span>
        </span>
        <a href="https://t.me/ov_linz" target="_blank" rel="noopener">
          JOIN
        </a>
      </div>
      <div className="box">
        <span className="icon">📅</span>
        <span className="text">
          Add showings to your calendar
          <span className="sub">Subscribe in Google, Apple or Outlook Calendar.</span>
        </span>
        <a href="/showings.ics">SUBSCRIBE</a>
      </div>
    </aside>
  );
}
```

Create `frontend/src/components/MovieCard.tsx`:

```tsx
import type { MovieView } from "../types";

export function MovieCard({ movie }: { movie: MovieView }) {
  return (
    <div className="card">
      <div className="filmrow">
        {movie.poster && <img src={`/posters/${movie.poster}`} alt="" loading="lazy" />}
        <div className="filmtitle">
          <strong>{movie.title}</strong>
          {movie.badge && <span className="badge">{movie.badge}</span>}
          {movie.metaLine && <div className="filmmeta">{movie.metaLine}</div>}
        </div>
      </div>
      {movie.showings.map((s, i) => (
        <a className="showing" href={s.url} key={i}>
          <span className="when">
            {s.date} · {s.time}
          </span>
          {s.detail && <span className="detail">{s.detail}</span>}
        </a>
      ))}
    </div>
  );
}
```

Create `frontend/src/components/CinemaSection.tsx`:

```tsx
import type { CinemaView } from "../types";
import { MovieCard } from "./MovieCard";

export function CinemaSection({ cinema }: { cinema: CinemaView }) {
  return (
    <section>
      <h2>{cinema.name}</h2>
      {cinema.movies.map((m) => (
        <MovieCard key={m.title} movie={m} />
      ))}
    </section>
  );
}
```

Create `frontend/src/App.tsx`:

```tsx
import { useEffect, useState } from "react";
import { fetchShowings } from "./api";
import type { ApiPayload } from "./types";
import { Marquee } from "./components/Marquee";
import { Sidebar } from "./components/Sidebar";
import { CinemaSection } from "./components/CinemaSection";

const POLL_MS = 15 * 60 * 1000; // mirrors the old <meta refresh=900>

export default function App() {
  const [payload, setPayload] = useState<ApiPayload | null>(null);

  useEffect(() => {
    let alive = true;
    const load = () =>
      fetchShowings()
        .then((p) => alive && setPayload(p))
        .catch(() => {});
    load();
    const id = setInterval(load, POLL_MS);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);

  return (
    <>
      <Marquee />
      <div className="layout">
        <Sidebar />
        <main>
          {payload === null || payload.cinemas === null ? (
            <p className="empty">No data yet — the first check is running.</p>
          ) : payload.cinemas.length === 0 ? (
            <p className="empty">No OV showings found right now.</p>
          ) : (
            payload.cinemas.map((c) => <CinemaSection key={c.name} cinema={c} />)
          )}
        </main>
      </div>
      {payload?.generatedAt && (
        <p className="meta">
          Last checked: {payload.generatedAt}
          {payload.sources && (
            <>
              {" · "}Cineplexx:{" "}
              <span className={payload.sources.cineplexx === "ok" ? "ok" : "err"}>
                {payload.sources.cineplexx ?? "–"}
              </span>
              {" · "}Megaplex:{" "}
              <span className={payload.sources.megaplex === "ok" ? "ok" : "err"}>
                {payload.sources.megaplex ?? "–"}
              </span>
            </>
          )}
        </p>
      )}
    </>
  );
}
```

Create `frontend/src/index.css` — the exact stylesheet from `app/web.py`'s template (note: `.layout main` margin rules and the media query included; this is the entire `<style>` block, copied verbatim):

```css
 :root{
  --bg:#0f0c09;--panel:#1c1611;--edge:#3a2f22;
  --gold:#e8b34d;--gold-bright:#f5c56b;
  --text:#f3ead9;--dim:#a89a83;--faint:#7d7160;
  --ok:#8fc98f;--err:#e07a6a;
 }
 *{box-sizing:border-box}
 body{
  background:var(--bg);
  background-image:radial-gradient(ellipse at 50% -10%,#241b10 0%,var(--bg) 60%);
  color:var(--text);
  font-family:system-ui,-apple-system,sans-serif;
   max-width:860px;margin:0 auto;padding:1.5rem 1rem 2.5rem;
 }
 .marquee{
  border:2px solid var(--gold);border-radius:10px;
  background:#171209;
  box-shadow:0 0 24px rgba(232,179,77,.25),inset 0 0 30px rgba(232,179,77,.08);
  padding:.7rem 1rem .9rem;text-align:center;margin-bottom:1.8rem;
 }
 .marquee h1{
  font-family:'Limelight',system-ui,sans-serif;font-weight:400;
  color:var(--gold-bright);font-size:2rem;letter-spacing:.18em;
  margin:.4rem 0 .25rem;
  text-shadow:0 0 12px rgba(245,197,107,.55),0 0 34px rgba(232,179,77,.3);
 }
 .tagline{
  color:var(--dim);font-size:.75rem;letter-spacing:.35em;
  text-transform:uppercase;margin:0 0 .3rem;
 }
 .bulbs{
  height:10px;
  background-image:radial-gradient(circle,var(--gold-bright) 1.6px,rgba(232,179,77,.15) 2.6px,transparent 3px);
  background-size:22px 10px;background-position:center;background-repeat:repeat-x;
  filter:drop-shadow(0 0 4px rgba(245,197,107,.8));
 }
 h2{
  color:var(--gold);font-size:.95rem;letter-spacing:.22em;
  text-transform:uppercase;margin:1.8rem 0 .7rem;padding-bottom:.4rem;
  border-bottom:double 3px var(--edge);
 }
 .card{
  position:relative;
  background:var(--panel);border:1px solid var(--edge);border-radius:8px;
  padding:.7rem 1rem .8rem 2.1rem;margin:.6rem 0;
 }
 .card::before{
  content:"";position:absolute;left:.5rem;top:.6rem;bottom:.6rem;width:10px;
  border-radius:2px;background-color:#2a2117;
  background-image:radial-gradient(circle at 50% 50%,var(--bg) 1.7px,transparent 2.4px);
  background-size:10px 14px;
 }
  .card strong{
   font-family:'Limelight',system-ui,sans-serif;font-weight:400;
   font-size:1.15rem;letter-spacing:.06em;
  }
  .filmrow{display:flex;gap:.8rem;align-items:center}
  .filmrow img{width:58px;border-radius:4px;border:1px solid var(--edge);flex:0 0 auto}
  .filmtitle{min-width:0}
  .filmmeta{color:var(--dim);font-size:.8rem;margin-top:.15rem}
 .badge{
  display:inline-block;background:var(--gold);color:#221a0c;border-radius:3px;
  padding:.05rem .45rem;font-size:.7rem;font-weight:700;letter-spacing:.12em;
  margin-left:.6rem;vertical-align:.15em;
  box-shadow:0 0 8px rgba(232,179,77,.35);
 }
 a{color:var(--gold-bright);text-decoration:none}
 a.showing{
  display:flex;align-items:baseline;gap:.6rem;
  color:var(--text);
  padding:.35rem .55rem;margin-top:.35rem;
  border:1px dashed var(--edge);border-radius:5px;
  transition:transform .12s ease,box-shadow .12s ease,border-color .12s ease;
 }
 a.showing:hover{
  transform:translateY(-1px);
  background:#231c12;border-color:var(--gold);
  box-shadow:0 2px 14px rgba(232,179,77,.25);
 }
 a.showing .when{
  color:var(--gold-bright);display:inline-block;min-width:9.5rem;
  font-variant-numeric:tabular-nums;letter-spacing:.04em;
 }
 a.showing .detail{color:var(--dim);font-size:.85rem}
.empty{
   text-align:center;color:var(--dim);
   border:1px dashed var(--edge);border-radius:8px;
   padding:2rem 1rem;margin:1.5rem 0;
  }
  .layout{display:flex;gap:1.2rem;align-items:flex-start}
  .layout main{flex:1;min-width:0}
  .layout main h2:first-child{margin-top:.2rem}
  .sidebar{flex:0 0 170px;position:sticky;top:1rem;
   display:flex;flex-direction:column;gap:.8rem}
  .sidebar .box{
   border:1px solid var(--edge);border-radius:8px;
   background:linear-gradient(180deg,#1a1410,#171109);
   padding:.9rem .8rem;
   display:flex;flex-direction:column;align-items:center;gap:.5rem;
   text-align:center;
  }
  .sidebar .box .icon{display:inline-flex;align-items:center;justify-content:center;
   width:28px;height:28px;flex:0 0 auto;font-size:1.4rem}
  .sidebar .box .icon svg{width:22px;height:22px;display:block}
  .sidebar .box .icon.tg{filter:drop-shadow(0 0 6px rgba(34,158,217,.4))}
  .sidebar .box .text{color:var(--text);font-size:.88rem}
  .sidebar .box .text .sub{color:var(--dim);font-size:.72rem;display:block;margin-top:.1rem}
  .sidebar .box a{width:100%;text-align:center;color:#221a0c;background:var(--gold);
   border-radius:4px;padding:.35rem .7rem;font-size:.75rem;font-weight:700;
   letter-spacing:.08em;box-shadow:0 0 8px rgba(232,179,77,.35)}
  .sidebar .box a:hover{background:var(--gold-bright)}
  @media (max-width:560px){
   .layout{flex-direction:column}
   .sidebar{position:static;flex-direction:column}
   .sidebar .box{flex-direction:row;text-align:left;padding:.7rem .75rem;gap:.4rem}
   .sidebar .box .icon{width:22px;height:22px;font-size:1.2rem}
   .sidebar .box .text{font-size:.82rem;flex:1}
   .sidebar .box a{width:auto;padding:.3rem .6rem;font-size:.7rem}
  }
  .meta{color:var(--faint);font-size:.8rem;margin-top:2rem;text-align:center}
 .ok{color:var(--ok)}.err{color:var(--err)}
```

- [ ] **Step 4: Run tests and build, then commit**

Run: `cd frontend && npx vitest run && npm run build`
Expected: 8 passed; build produces `frontend/dist/`.

```bash
git add frontend/
git commit -m "Add React components with ported cinema styling"
```

---

### Task 13: `import-state` subcommand (cutover from JSON state)

**Files:**
- Create: `backend/src/import.rs`
- Modify: `backend/src/main.rs` (add `mod import;` + subcommand dispatch)

**Interfaces:**
- Consumes: `db::*` (Task 2), `models::MovieMeta` (Task 3).
- Produces: `import::run(pool: &PgPool, data_dir: &Path) -> anyhow::Result<()>` — reads `<data_dir>/showings.json` and `<data_dir>/state.json`, seeds movies/showings/source statuses/check_run. Missing `showings.json` → logs and exits `Ok(())`.

Import semantics:
- Each `showings[]` entry: upsert movie (metadata from `movies["Cinema|Title"]` if present), insert showing with `first_seen_at` = `state.json`'s `seen["Cinema|Title|<start>"]` timestamp (fallback: `generated_at`). Duplicates are skipped via `ON CONFLICT DO NOTHING`.
- `sources` map → `source_status` rows (status only); `state.json`'s `error_pings` → `last_error_ping_date`.
- One `check_run` row: `(generated_at, 0, len(showings))` so the footer timestamp stays continuous.

- [ ] **Step 1: Write the failing test**

Create `backend/src/import.rs`:

```rust
use sqlx::PgPool;
use std::path::Path;

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
            DateTime::parse_from_rfc3339("2026-08-01T00:00:00+00:00").unwrap().with_timezone(&Utc),
        )
        .await
        .unwrap();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].movie, "The Odyssey");
        assert_eq!(views[0].runtime_min, Some(180));
        assert_eq!(views[0].poster_file.as_deref(), Some("a1b2.jpg"));

        // dedup preserved: re-import inserts nothing new
        run(&pool, dir.path()).await.unwrap();
        let mid = crate::db::upsert_movie(&pool, "Cineplexx Linz", "The Odyssey", None, &[], None, None).await.unwrap();
        let inserted = crate::db::insert_showing(
            &pool,
            mid,
            DateTime::parse_from_rfc3339("2026-08-04T19:30:00+02:00").unwrap().with_timezone(&Utc),
            "OV",
            "Saal 6",
            "https://cineplexx.at/film/die-odyssee",
            Utc::now(),
        )
        .await
        .unwrap();
        assert!(!inserted, "imported showing must be treated as seen");

        // source statuses incl. error ping date
        let (status, ping) = crate::db::get_source_status(&pool, "megaplex").await.unwrap().unwrap();
        assert_eq!(status, "error");
        assert_eq!(ping, Some(NaiveDate::from_ymd_opt(2026, 8, 1).unwrap()));
        let (status, _) = crate::db::get_source_status(&pool, "cineplexx").await.unwrap().unwrap();
        assert_eq!(status, "ok");

        // check run seeded with generated_at
        let latest = crate::db::latest_check_run(&pool).await.unwrap().unwrap();
        assert_eq!(
            latest,
            DateTime::parse_from_rfc3339("2026-08-01T12:00:00+02:00").unwrap().with_timezone(&Utc)
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn missing_showings_json_is_a_noop(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        run(&pool, dir.path()).await.unwrap();
        assert!(crate::db::latest_check_run(&pool).await.unwrap().is_none());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path backend/Cargo.toml import`
Expected: FAIL to compile — `run` not found. (Add `mod import;` to `main.rs`.)

- [ ] **Step 3: Implement the importer**

Add to `backend/src/import.rs` (before the test module):

```rust
use chrono::{DateTime, NaiveDate, Utc};

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
            .map(|a| a.iter().filter_map(|g| g.as_str().map(String::from)).collect())
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
```

Wire the subcommand in `backend/src/main.rs` — add `mod import;` and, directly after running migrations:

```rust
    if std::env::args().nth(1).as_deref() == Some("import-state") {
        let dir = std::env::args()
            .nth(2)
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| config.data_dir.clone());
        import::run(&pool, &dir).await?;
        return Ok(());
    }
```

- [ ] **Step 4: Run tests, then commit**

Run: `cargo test --manifest-path backend/Cargo.toml`
Expected: all passed (56 total).

```bash
git add backend/
git commit -m "Add import-state subcommand for JSON cutover"
```

---

### Task 14: Dockerfile + compose app service

**Files:**
- Modify: `Dockerfile` (full rewrite)
- Create: `.dockerignore`
- Modify: `docker-compose.yml` (add `app` service)

**Interfaces:**
- Consumes: `backend/` (Task 1–13), `frontend/` (Task 11–12). Runtime env: `DATABASE_URL`, `STATIC_DIR=/srv/static`, `DATA_DIR=/data`, `PORT=8080`.

- [ ] **Step 1: Write the Dockerfile and .dockerignore**

Replace `Dockerfile`:

```dockerfile
# frontend build
FROM node:22-alpine AS frontend
WORKDIR /build
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

# backend build
FROM rust:1-slim-bookworm AS backend
WORKDIR /build
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/migrations ./migrations
COPY backend/src ./src
RUN cargo build --release

# runtime
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 10001 ov \
    && mkdir -p /data \
    && chown ov /data
COPY --from=backend /build/target/release/ov-watcher /usr/local/bin/ov-watcher
COPY --from=frontend /build/dist /srv/static
USER ov
ENV DATA_DIR=/data PORT=8080 STATIC_DIR=/srv/static
EXPOSE 8080
CMD ["ov-watcher"]
```

Create `.dockerignore`:

```
**/target
**/node_modules
frontend/dist
.venv
data
.git
docs
.pytest_cache
```

- [ ] **Step 2: Extend docker-compose.yml with the app service**

Replace `docker-compose.yml`:

```yaml
services:
  db:
    image: postgres:17-alpine
    environment:
      POSTGRES_USER: ov
      POSTGRES_PASSWORD: ov
      POSTGRES_DB: ov
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U ov"]
      interval: 2s
      timeout: 3s
      retries: 20

  app:
    build: .
    depends_on:
      db:
        condition: service_healthy
    environment:
      DATABASE_URL: postgres://ov:ov@db:5432/ov
      TELEGRAM_BOT_TOKEN: ${TELEGRAM_BOT_TOKEN:-}
      TELEGRAM_CHAT_ID: ${TELEGRAM_CHAT_ID:-}
    ports:
      - "8080:8080"
    volumes:
      - ov-data:/data

volumes:
  pgdata: {}
  ov-data: {}
```

- [ ] **Step 3: Verify the image builds and the stack responds**

```bash
docker compose build
docker compose up -d
curl -fsS http://localhost:8080/healthz          # -> ok
curl -fsS http://localhost:8080/api/showings     # -> {"generatedAt":null,"sources":null,"cinemas":null}
curl -fsS http://localhost:8080/showings.ics     # -> BEGIN:VCALENDAR...
curl -fsS http://localhost:8080/ | head -5       # -> index.html (React app)
docker compose down
```

(The first check run will fail with fetch errors if run without internet access to the cinemas — that only flips `sources` to `error`, the API still answers. Telegram is skipped because the env vars are empty.)

- [ ] **Step 4: Commit**

```bash
git add Dockerfile .dockerignore docker-compose.yml
git commit -m "Add multi-stage Dockerfile and compose app service"
```

---

### Task 15: Deployment — helm chart, k8s manifests, CI

**Files:**
- Modify: `helm/ov-watcher/values.yaml`
- Modify: `helm/ov-watcher/templates/secret.yaml`
- Create: `helm/ov-watcher/templates/postgres.yaml`
- Modify: `k8s/secret.example.yaml`
- Create: `k8s/postgres.yaml`
- Modify: `.github/workflows/deploy.yml` (add `test` job; pass `postgres.password`)

**Interfaces:**
- Consumes: Task 14's image (env `DATABASE_URL`). New helm values: `postgres.create` (default `true`), `postgres.password` (required when `postgres.create`), `postgres.storage` (default `1Gi`), `secrets.databaseUrl` (used when `postgres.create=false`).

- [ ] **Step 1: Update the helm chart**

Update `helm/ov-watcher/values.yaml` — append:

```yaml
postgres:
  create: true
  password: ""        # set once via --set, must stay stable afterwards
  storage: 1Gi

secrets:
  databaseUrl: ""     # only used when postgres.create=false
```

Replace `helm/ov-watcher/templates/secret.yaml`:

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: ov-watcher-secret
type: Opaque
stringData:
  TELEGRAM_BOT_TOKEN: "{{ .Values.secrets.telegramBotToken }}"
  TELEGRAM_CHAT_ID: "{{ .Values.secrets.telegramChatId }}"
{{- if .Values.postgres.create }}
  POSTGRES_PASSWORD: "{{ required "postgres.password is required" .Values.postgres.password }}"
  DATABASE_URL: "postgres://ov:{{ .Values.postgres.password }}@ov-watcher-postgres:5432/ov"
{{- else }}
  DATABASE_URL: "{{ required "secrets.databaseUrl is required when postgres.create=false" .Values.secrets.databaseUrl }}"
{{- end }}
```

Create `helm/ov-watcher/templates/postgres.yaml`:

```yaml
{{- if .Values.postgres.create }}
apiVersion: v1
kind: Service
metadata:
  name: ov-watcher-postgres
  labels:
    app: ov-watcher-postgres
spec:
  clusterIP: None
  selector:
    app: ov-watcher-postgres
  ports:
    - port: 5432
---
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: ov-watcher-postgres
  labels:
    app: ov-watcher-postgres
spec:
  serviceName: ov-watcher-postgres
  replicas: 1
  selector:
    matchLabels:
      app: ov-watcher-postgres
  template:
    metadata:
      labels:
        app: ov-watcher-postgres
    spec:
      containers:
        - name: postgres
          image: postgres:17-alpine
          ports:
            - containerPort: 5432
          env:
            - name: POSTGRES_USER
              value: ov
            - name: POSTGRES_DB
              value: ov
            - name: POSTGRES_PASSWORD
              valueFrom:
                secretKeyRef:
                  name: ov-watcher-secret
                  key: POSTGRES_PASSWORD
          volumeMounts:
            - name: pgdata
              mountPath: /var/lib/postgresql/data
          resources:
            requests:
              cpu: 50m
              memory: 128Mi
            limits:
              cpu: 500m
              memory: 512Mi
  volumeClaimTemplates:
    - metadata:
        name: pgdata
      spec:
        accessModes: ["ReadWriteOnce"]
        resources:
          requests:
            storage: {{ .Values.postgres.storage }}
{{- end }}
```

- [ ] **Step 2: Update the plain k8s manifests**

Update `k8s/secret.example.yaml` — add to `stringData`:

```yaml
  POSTGRES_PASSWORD: "change-me"
  DATABASE_URL: "postgres://ov:change-me@ov-watcher-postgres:5432/ov"
```

Create `k8s/postgres.yaml`:

```yaml
apiVersion: v1
kind: Service
metadata:
  name: ov-watcher-postgres
  labels:
    app: ov-watcher-postgres
spec:
  clusterIP: None
  selector:
    app: ov-watcher-postgres
  ports:
    - port: 5432
---
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: ov-watcher-postgres
  labels:
    app: ov-watcher-postgres
spec:
  serviceName: ov-watcher-postgres
  replicas: 1
  selector:
    matchLabels:
      app: ov-watcher-postgres
  template:
    metadata:
      labels:
        app: ov-watcher-postgres
    spec:
      containers:
        - name: postgres
          image: postgres:17-alpine
          ports:
            - containerPort: 5432
          env:
            - name: POSTGRES_USER
              value: ov
            - name: POSTGRES_DB
              value: ov
            - name: POSTGRES_PASSWORD
              valueFrom:
                secretKeyRef:
                  name: ov-watcher-secret
                  key: POSTGRES_PASSWORD
          volumeMounts:
            - name: pgdata
              mountPath: /var/lib/postgresql/data
          resources:
            requests:
              cpu: 50m
              memory: 128Mi
            limits:
              cpu: 500m
              memory: 512Mi
  volumeClaimTemplates:
    - metadata:
        name: pgdata
      spec:
        accessModes: ["ReadWriteOnce"]
        resources:
          requests:
            storage: 1Gi
```

- [ ] **Step 3: Update CI**

In `.github/workflows/deploy.yml`, insert a new `test` job **before** the `build` job, and add `needs: test` to `build`:

```yaml
jobs:
  test:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:17-alpine
        env:
          POSTGRES_USER: ov
          POSTGRES_PASSWORD: ov
          POSTGRES_DB: ov
        ports:
          - 5432:5432
        options: >-
          --health-cmd "pg_isready -U ov"
          --health-interval 5s
          --health-timeout 5s
          --health-retries 10
    env:
      DATABASE_URL: postgres://ov:ov@localhost:5432/ov
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt

      - name: Cache cargo
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: backend

      - name: Backend tests
        working-directory: backend
        run: |
          cargo fmt --check
          cargo clippy -- -D warnings
          cargo test

      - name: Set up Node
        uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm
          cache-dependency-path: frontend/package-lock.json

      - name: Frontend tests
        working-directory: frontend
        run: |
          npm ci
          npm test
          npm run build
```

Then in the same file: change `build:` to include `needs: test`, and in the `deploy` job's `helm upgrade --install` command add:

```
            --set postgres.password="${{ secrets.POSTGRES_PASSWORD }}" \
```

(One-time manual step, noted in the PR description: add `POSTGRES_PASSWORD` to the repo secrets; keep it stable across deploys — changing it later would desync the initialized Postgres volume.)

- [ ] **Step 4: Verify and commit**

```bash
helm template ov-watcher ./helm/ov-watcher --set postgres.password=test --set secrets.telegramBotToken=x > /dev/null
cargo fmt --check --manifest-path backend/Cargo.toml
git add helm/ k8s/ .github/
git commit -m "Add Postgres to deployment and CI test job"
```

---

### Task 16: Remove the Python app, update docs, final verification

**Files:**
- Move: `tests/fixtures/` → `backend/tests/fixtures/`
- Delete: `app/`, `tests/` (remaining), `requirements.txt`, `requirements-dev.txt`, `serve.sh`
- Modify: `backend/src/fetchers/mod.rs` (fixture path)
- Modify: `README.md`, `AGENTS.md`

- [ ] **Step 1: Move the fixtures and update the Rust fixture helper**

```bash
mkdir -p backend/tests
git mv tests/fixtures backend/tests/fixtures
```

In `backend/src/fetchers/mod.rs`, change the `fixture` helper to:

```rust
#[cfg(test)]
pub(crate) fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(path).unwrap()
}
```

Run: `cargo test --manifest-path backend/Cargo.toml`
Expected: all passed.

- [ ] **Step 2: Delete the Python app**

```bash
git rm -r app tests requirements.txt requirements-dev.txt serve.sh
git commit -m "Remove Python implementation superseded by the Rust rewrite"
```

- [ ] **Step 3: Update README.md and AGENTS.md**

Replace `README.md` content — same German tone as before, new commands:

````markdown
# Cinema OV Watcher

Findet neue OV/OmU-Vorstellungen (englische Originalfassungen) im
Cineplexx Linz und Hollywood Megaplex PlusCity, postet Telegram-Alerts
im öffentlichen Kanal [@ov_linz](https://t.me/ov_linz) und zeigt alle
kommenden OV-Vorstellungen auf einer Webseite.

Rust-Backend (axum + Postgres) mit React-Frontend. Laufzeit, Genre und
Filmplakat werden direkt von den Kinoseiten gelesen und auf der Webseite,
in den Telegram-Alerts und im Kalender-Feed (`/showings.ics`) angezeigt.

## Lokal laufen lassen

```bash
docker compose up -d db
export DATABASE_URL=postgres://ov:ov@localhost:5432/ov
cd backend && cargo run            # http://localhost:8080
cd frontend && npm install && npm run dev   # Dev-Server mit Proxy (optional)
```

Telegram-Bot: bei @BotFather anlegen, Token notieren. Der Bot postet im
öffentlichen Kanal @ov_linz: dazu den Bot im Kanal als Administrator mit
Recht „Nachrichten posten" hinzufügen und als `TELEGRAM_CHAT_ID` einfach
`@ov_linz` eintragen.

## Tests

```bash
cd backend && cargo test           # braucht DATABASE_URL (docker compose up -d db)
cd frontend && npm test
```

## Docker

```bash
docker compose up --build          # App + Postgres, http://localhost:8080
```

## Kubernetes

```bash
kubectl apply -f k8s/pvc.yaml -f k8s/postgres.yaml -f k8s/secret.yaml \
  -f k8s/configmap.yaml -f k8s/deployment.yaml -f k8s/service.yaml
```

`k8s/secret.yaml` aus `k8s/secret.example.yaml` erzeugen und die echten
Werte eintragen (nicht committen).
````

Update `AGENTS.md` — replace the Layout/Running/Tests sections:

- Layout: `backend/` (Rust: `models.rs`, `fetchers/`, `checker.rs`, `notify.rs`, `ics.rs`, `db.rs`, `web.rs`, `import.rs`, `main.rs`), `frontend/` (React + Vite), state in Postgres, posters in `DATA_DIR/posters/`.
- Running locally: `docker compose up -d db`, `DATABASE_URL=... cargo run` in `backend/`; optional Vite dev server.
- Tests: `cargo test` (needs `DATABASE_URL`; `#[sqlx::test]` creates per-test DBs) and `npm test` in `frontend/`.
- Cutover: `ov-watcher import-state <data_dir>` seeds the DB from old JSON state.
- Remove the `serve.sh` sections entirely (both the "Running the web UI locally" block and the "Background jobs via the bash tool" note that recommends `serve.sh`).

- [ ] **Step 4: Final end-to-end verification**

```bash
cargo test --manifest-path backend/Cargo.toml
cd frontend && npm test && npm run build && cd ..
docker compose build && docker compose up -d
curl -fsS http://localhost:8080/healthz
curl -fsS http://localhost:8080/api/showings
curl -fsS http://localhost:8080/showings.ics
docker compose down
```

Expected: all green; `/` serves the React app; API answers the null-state until the first successful check run.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "Update docs for the Rust + React rewrite"
```

---

## Cutover runbook (post-merge, manual)

1. Add `POSTGRES_PASSWORD` to the GitHub repo secrets (random, then never changed).
2. Merge to `master` — CI runs tests, builds the image, deploys via helm.
3. Before the first pod with the new image starts, or immediately after:
   `kubectl exec deploy/ov-watcher -- ov-watcher import-state /data` (the PVC still
   holds `showings.json`/`state.json` from the Python app) — seeds the DB so the
   first check doesn't re-notify existing showings. Safe to skip: worst case is
   one duplicate Telegram message.
4. Verify: `kubectl logs deploy/ov-watcher` shows a finished check;
   `https://cinema.k-labs.app/` renders the React app; `/showings.ics` has events.




