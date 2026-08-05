use crate::db::ShowingView;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: String,
    pub from: String,
}

#[derive(Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Clone)]
pub struct AppleConfig {
    pub client_id: String,
    pub team_id: String,
    pub key_id: String,
    pub private_key: String,
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub data_dir: PathBuf,
    pub static_dir: PathBuf,
    pub base_url: String,
    pub smtp_config: Option<SmtpConfig>,
    pub google_oauth: Option<OAuthConfig>,
    pub apple_oauth: Option<AppleConfig>,
    pub github_oauth: Option<OAuthConfig>,
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
    pub start: String,
    pub detail: String,
    pub url: String,
}

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use axum::Router;
use chrono_tz::Europe::Vienna;
use std::collections::HashSet;
use tower_http::services::{ServeDir, ServeFile};

const CINEMA_ORDER: [&str; 1] = ["Megaplex PlusCity"];

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/showings", get(api_showings))
        .route("/showings.ics", get(showings_ics))
        .route("/posters/{name}", get(poster))
        .route("/healthz", get(healthz))
        .merge(crate::auth::auth_router())
        .fallback_service(
            ServeDir::new(&state.static_dir)
                .fallback(ServeFile::new(state.static_dir.join("index.html"))),
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
        None => Ok(Json(ApiPayload {
            generated_at: None,
            sources: None,
            cinemas: None,
        })),
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
    (
        [(header::CONTENT_TYPE, "text/calendar; charset=utf-8")],
        body,
    )
        .into_response()
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
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "max-age=86400"),
        ],
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

type MovieGroup<'a> = (String, Vec<&'a ShowingView>);
type CinemaGroup<'a> = (String, Vec<MovieGroup<'a>>);

pub fn build_payload(
    run_at: DateTime<Utc>,
    statuses: Vec<(String, String)>,
    views: Vec<ShowingView>,
) -> ApiPayload {
    // group by cinema, then movie, preserving order of first appearance
    // (the query is already sorted by start, cinema)
    let mut cinemas: Vec<CinemaGroup> = Vec::new();
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
            CINEMA_ORDER
                .iter()
                .position(|c| c == name)
                .unwrap_or(CINEMA_ORDER.len()),
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
            run_at
                .with_timezone(&Vienna)
                .format("%Y-%m-%dT%H:%M:%S%:z")
                .to_string(),
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
    let mut meta_parts: Vec<String> = Vec::new();
    if !first.genres.is_empty() {
        meta_parts.push(first.genres.join(", "));
    }
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
                None => parts.push(if variant.is_empty() {
                    s.version.clone()
                } else {
                    variant.to_string()
                }),
                Some(_) if !variant.is_empty() => parts.push(variant.to_string()),
                _ => {}
            }
            if !s.hall.is_empty() {
                parts.push(s.hall.clone());
            }
            ShowingRow {
                start: local.format("%Y-%m-%dT%H:%M:%S%:z").to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use chrono_tz::Europe::Vienna;

    fn view(
        cinema: &str,
        movie: &str,
        day: u32,
        hour: u32,
        version: &str,
        hall: &str,
    ) -> ShowingView {
        ShowingView {
            cinema: cinema.into(),
            movie: movie.into(),
            start: Vienna
                .with_ymd_and_hms(2026, 8, day, hour, 30, 0)
                .unwrap()
                .with_timezone(&Utc),
            version: version.into(),
            hall: hall.into(),
            url: "https://x".into(),
            runtime_min: None,
            genres: vec![],
            poster_file: None,
        }
    }

    fn run_at() -> DateTime<Utc> {
        Vienna
            .with_ymd_and_hms(2026, 8, 2, 12, 0, 0)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn payload_none_states() {
        // tested at the handler level; build_payload always has data
        let p = ApiPayload {
            generated_at: None,
            sources: None,
            cinemas: None,
        };
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"generatedAt": null, "sources": null, "cinemas": null})
        );
    }

    #[test]
    fn groups_showings_and_formats_rows() {
        let mut odyssey = view("Cineplexx Linz", "The Odyssey", 4, 19, "OV", "Saal 7");
        odyssey.runtime_min = Some(121);
        odyssey.genres = vec!["Abenteuer".into(), "Historie".into()];
        odyssey.poster_file = Some("a.jpg".into());
        let views = vec![
            odyssey,
            view(
                "Megaplex PlusCity",
                "Die Odyssee",
                3,
                20,
                "OV - IMAX 2D",
                "",
            ),
        ];
        let p = build_payload(run_at(), vec![("cineplexx".into(), "ok".into())], views);
        assert_eq!(p.generated_at.as_deref(), Some("2026-08-02T12:00:00+02:00"));
        // Megaplex first despite later in the alphabet
        assert_eq!(p.cinemas.as_ref().unwrap()[0].name, "Megaplex PlusCity");
        let cineplexx = &p.cinemas.as_ref().unwrap()[1];
        let m = &cineplexx.movies[0];
        assert_eq!(m.title, "The Odyssey");
        assert_eq!(m.badge.as_deref(), Some("OV"));
        assert_eq!(m.meta_line, "Abenteuer, Historie · 121 Min");
        assert_eq!(m.poster.as_deref(), Some("a.jpg"));
        assert_eq!(m.showings[0].start, "2026-08-04T19:30:00+02:00");
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
            view(
                "Megaplex PlusCity",
                "Die Odyssee",
                3,
                19,
                "OV - IMAX 2D",
                "",
            ),
            view(
                "Megaplex PlusCity",
                "Die Odyssee",
                4,
                20,
                "OV - Dolby Vision 2D",
                "",
            ),
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

        let state = AppState {
            pool: pool.clone(),
            data_dir: PathBuf::new(),
            static_dir: PathBuf::from("/nonexistent"),
            base_url: "http://localhost".into(),
            smtp_config: None,
            google_oauth: None,
            apple_oauth: None,
            github_oauth: None,
        };
        let app = router(state);
        // state 1: no check run yet -> nulls
        let resp = app
            .clone()
            .oneshot(
                Request::get("/api/showings")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["cinemas"], serde_json::Value::Null);
        // state 2: check run, no showings -> []
        crate::db::insert_check_run(&pool, Utc::now(), 0, 0)
            .await
            .unwrap();
        let resp = app
            .clone()
            .oneshot(
                Request::get("/api/showings")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["cinemas"], serde_json::json!([]));
        // state 3: data present
        let mid = crate::db::upsert_movie(&pool, "Cineplexx Linz", "F1", None, &[], None, None)
            .await
            .unwrap();
        crate::db::insert_showing(
            &pool,
            mid,
            Utc::now() + chrono::Duration::days(1),
            "OV",
            "Saal 6",
            "https://x",
            Utc::now(),
        )
        .await
        .unwrap();
        let resp = app
            .oneshot(
                Request::get("/api/showings")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["cinemas"][0]["movies"][0]["title"], "F1");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn ics_route_renders_events(pool: PgPool) {
        use axum::body::to_bytes;
        use axum::http::Request;
        use tower::ServiceExt;

        let mid =
            crate::db::upsert_movie(&pool, "Cineplexx Linz", "F1", Some(121), &[], None, None)
                .await
                .unwrap();
        crate::db::insert_showing(
            &pool,
            mid,
            Utc::now() + chrono::Duration::days(1),
            "OV",
            "Saal 6",
            "https://x",
            Utc::now(),
        )
        .await
        .unwrap();
        let state = AppState {
            pool,
            data_dir: PathBuf::new(),
            static_dir: PathBuf::from("/nonexistent"),
            base_url: "http://localhost".into(),
            smtp_config: None,
            google_oauth: None,
            apple_oauth: None,
            github_oauth: None,
        };
        let resp = router(state)
            .oneshot(
                Request::get("/showings.ics")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
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
        let state = AppState {
            pool,
            data_dir: dir.path().to_path_buf(),
            static_dir: PathBuf::from("/nonexistent"),
            base_url: "http://localhost".into(),
            smtp_config: None,
            google_oauth: None,
            apple_oauth: None,
            github_oauth: None,
        };
        let app = router(state);
        let resp = app
            .clone()
            .oneshot(
                Request::get("/posters/a1b2.jpg")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers()["cache-control"].to_str().unwrap(),
            "max-age=86400"
        );
        assert_eq!(
            resp.headers()["content-type"].to_str().unwrap(),
            "image/jpeg"
        );
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"img");
        // traversal / dotfile attempts are rejected
        for bad in ["..", ".hidden", "..%2Fetc"] {
            let resp = app
                .clone()
                .oneshot(
                    Request::get(format!("/posters/{bad}"))
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), 404, "expected 404 for {bad}");
        }
        let resp = app
            .oneshot(
                Request::get("/posters/missing.jpg")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 404);
    }

    #[tokio::test]
    async fn healthz_route() {
        use axum::body::to_bytes;
        use axum::http::Request;
        use tower::ServiceExt;

        let pool = PgPool::connect_lazy("postgres://ov:ov@localhost/ov").unwrap();
        let state = AppState {
            pool,
            data_dir: PathBuf::new(),
            static_dir: PathBuf::from("/nonexistent"),
            base_url: "http://localhost".into(),
            smtp_config: None,
            google_oauth: None,
            apple_oauth: None,
            github_oauth: None,
        };
        let resp = router(state)
            .oneshot(
                Request::get("/healthz")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"ok");
    }
}
