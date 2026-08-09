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
        .and_then(|seg| {
            seg.rsplit_once('.')
                .map(|(_, e)| format!(".{}", e.to_lowercase()))
        })
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
        if target.exists() {
            cached.insert(key.clone(), Some(name));
        } else {
            // write/rename failed -> don't reference a missing file
            cached.insert(key.clone(), None);
        }
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
    let upcoming: Vec<Showing> = all_showings
        .into_iter()
        .filter(|s| s.start >= now)
        .collect();
    let wanted: HashSet<String> = upcoming
        .iter()
        .map(|s| format!("{}|{}", s.cinema, s.movie))
        .collect();
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
        if db::insert_showing(
            ctx.pool, movie_id, s.start, &s.version, &s.hall, &s.url, now,
        )
        .await?
        {
            new_showings.push(s.clone());
        }
    }
    db::prune(ctx.pool, now - chrono::Duration::hours(6)).await?;
    db::insert_check_run(
        ctx.pool,
        now,
        new_showings.len() as i32,
        upcoming.len() as i32,
    )
    .await?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::MovieMeta;
    use chrono::TimeZone;
    use chrono_tz::Europe::Vienna;
    use std::sync::{Arc, Mutex};

    fn now() -> DateTime<Utc> {
        Vienna
            .with_ymd_and_hms(2026, 7, 18, 12, 0, 0)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn make_showing(day: u32) -> Showing {
        Showing {
            cinema: "Cineplexx Linz".into(),
            movie: "The Odyssey".into(),
            start: Vienna
                .with_ymd_and_hms(2026, 7, day, 19, 0, 0)
                .unwrap()
                .with_timezone(&Utc),
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
        CheckCtx {
            pool,
            http,
            config,
            notifier,
            fetchers: vec![("cineplexx", fetcher)],
        }
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
            smtp_host: None,
            smtp_port: 587,
            smtp_username: None,
            smtp_password: None,
            smtp_from: None,
            base_url: "https://cinema.k-labs.app".into(),
            google_client_id: None,
            google_client_secret: None,
            github_client_id: None,
            github_client_secret: None,
            fake_login: false,
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
        let fetcher = FakeFetcher {
            result: Ok((vec![make_showing(20)], HashMap::new())),
        };
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
        let fetcher = FakeFetcher {
            result: Ok((vec![make_showing(17)], HashMap::new())),
        };
        let notifier = RecordingNotifier::default();
        let c = ctx(&pool, &http, &cfg, Some(&notifier), &fetcher);
        let r = run_check(&c, now()).await.unwrap();
        assert_eq!(r.total_showings, 0);
        assert!(notifier.sent.lock().unwrap().is_empty());
        assert!(crate::db::upcoming_view(&pool, now())
            .await
            .unwrap()
            .is_empty());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn source_error_sends_rate_limited_ping(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(dir.path(), true);
        let http = http();
        let fetcher = FakeFetcher {
            result: Err("kaputt".to_string()),
        };
        let notifier = RecordingNotifier::default();
        let c = CheckCtx {
            pool: &pool,
            http: &http,
            config: &cfg,
            notifier: Some(&notifier),
            fetchers: vec![("megaplex", &fetcher)],
        };
        let r = run_check(&c, now()).await.unwrap();
        assert_eq!(
            r.sources,
            HashMap::from([("megaplex".to_string(), "error".to_string())])
        );
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
            (
                "Cineplexx Linz|The Odyssey".to_string(),
                MovieMeta {
                    runtime_min: Some(180),
                    genres: vec!["Abenteuer".into()],
                    poster: None,
                },
            ),
            (
                "Cineplexx Linz|Not Shown".to_string(),
                MovieMeta {
                    runtime_min: Some(90),
                    genres: vec![],
                    poster: None,
                },
            ),
        ]);
        let fetcher = FakeFetcher {
            result: Ok((vec![make_showing(20)], metas)),
        };
        let c = ctx(&pool, &http, &cfg, None, &fetcher);
        run_check(&c, now()).await.unwrap();
        let view = crate::db::upcoming_view(&pool, now()).await.unwrap();
        assert_eq!(view.len(), 1);
        assert_eq!(view[0].runtime_min, Some(180));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn posters_downloaded_referenced_and_pruned(pool: PgPool) {
        // local poster server serving two images
        let app = axum::Router::new().route(
            "/p.jpg",
            axum::routing::get(|| async { b"\xff\xd8img".as_slice() }),
        );
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
                MovieMeta {
                    runtime_min: Some(180),
                    genres: vec![],
                    poster: Some(poster_url.clone()),
                },
            ),
            // filtered out (no such showing) -> must not trigger a download
            (
                "Cineplexx Linz|Not Shown".to_string(),
                MovieMeta {
                    runtime_min: Some(90),
                    genres: vec![],
                    poster: Some(format!("http://{addr}/never.jpg")),
                },
            ),
        ]);
        let fetcher = FakeFetcher {
            result: Ok((vec![make_showing(20)], metas)),
        };
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
        assert_eq!(
            view[0].poster_file.as_deref(),
            Some(poster_filename(&poster_url).as_str())
        );

        // second run: file exists -> cache hit, still referenced (no download)
        run_check(&c, now()).await.unwrap();
        let view = crate::db::upcoming_view(&pool, now()).await.unwrap();
        assert_eq!(
            view[0].poster_file.as_deref(),
            Some(poster_filename(&poster_url).as_str())
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn poster_failure_is_best_effort(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(dir.path(), false);
        // no poster server running -> connection refused -> poster_file stays None
        let http = http();
        let metas = HashMap::from([(
            "Cineplexx Linz|The Odyssey".to_string(),
            MovieMeta {
                runtime_min: Some(180),
                genres: vec![],
                poster: Some("http://127.0.0.1:1/none.jpg".into()),
            },
        )]);
        let fetcher = FakeFetcher {
            result: Ok((vec![make_showing(20)], metas)),
        };
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
