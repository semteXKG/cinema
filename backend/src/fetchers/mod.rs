pub mod cineplexx;
pub mod megaplex;

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
        .join("tests/fixtures")
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

    mod megaplex_tests {
        use super::super::fixture;
        use super::super::megaplex::*;
        use chrono::Datelike;
        use chrono::NaiveDate;
        use chrono::Timelike;

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
            let html =
                "<html><body><h1>Other (Pluscity) - OV</h1>Aktuelles Kinoprogramm</body></html>";
            let (showings, metas) = parse_megaplex_film_page(html, "https://x", today()).unwrap();
            assert!(showings.is_empty());
            assert!(metas.is_empty());
        }

        #[test]
        fn parse_film_page_without_kinoprogramm_is_source_error() {
            let r =
                parse_megaplex_film_page("<html><body>garbage</body></html>", "https://x", today());
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
}
