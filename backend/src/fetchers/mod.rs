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
    use chrono::Datelike;
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
            crate::models::MovieMeta {
                runtime_min: None,
                genres: vec![],
                poster: None
            }
        );
    }
}
