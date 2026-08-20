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
            return Err(SourceError::msg(format!(
                "Cineplexx: invalid sessions for {id}"
            )));
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
        metas
            .entry(title.clone())
            .or_insert_with(|| cineplexx_meta(movie));
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
                let hall = session
                    .get("screenName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let features =
                    crate::models::extract_features(&cineplexx_feature_text(session, &version));
                showings.push(Showing {
                    cinema: CINEPLEXX_CINEMA_NAME.to_string(),
                    movie: title.clone(),
                    start: start.with_timezone(&Utc),
                    version,
                    hall,
                    url: url.clone(),
                    features,
                });
            }
        }
    }
    showings.sort_by_key(|s| s.start);
    (showings, metas)
}

pub fn cineplexx_feature_text(session: &serde_json::Value, version: &str) -> String {
    let mut parts: Vec<String> = vec![version.to_string()];
    if let Some(screen) = session.get("screenName").and_then(|v| v.as_str()) {
        parts.push(screen.to_string());
    }
    if let Some(groups) = session.get("technologies").and_then(|t| t.as_array()) {
        for group in groups.iter().filter_map(|g| g.as_array()) {
            for label in group.iter().filter_map(|l| l.as_str()) {
                parts.push(label.to_string());
            }
        }
    }
    if let Some(attrs) = session
        .get("conceptAttributesNames")
        .and_then(|a| a.as_array())
    {
        for attr in attrs.iter().filter_map(|a| a.as_str()) {
            parts.push(attr.to_string());
        }
    }
    parts.join(" ")
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
    MovieMeta {
        runtime_min,
        genres,
        poster,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cineplexx_features_from_session_arrays() {
        let session = json!({
            "technologies": [["2D", "OV (Englisch)", "IMAX"], []],
            "conceptAttributesNames": ["OV"],
            "screenName": "Saal 6"
        });
        let text = cineplexx_feature_text(&session, "OV");
        assert_eq!(
            crate::models::extract_features(&text),
            vec!["OV", "IMAX", "2D"]
        );
    }
}
