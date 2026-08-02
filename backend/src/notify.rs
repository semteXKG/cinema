use crate::models::{MovieMeta, Showing};
use chrono::Utc;
use std::collections::HashMap;

pub const MAX_LEN: usize = 4096;

use chrono::Datelike;
use std::collections::{BTreeMap, HashSet};
use std::fmt::Display;

const WEEKDAYS: [&str; 7] = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];

pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn escape_attr(s: &str) -> String {
    escape_html(s)
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

pub fn format_message(showings: &[Showing], movies: &HashMap<String, MovieMeta>) -> String {
    let mut lines = vec![
        "🎬 <b>Neue OV-Vorstellungen in Linz</b>".to_string(),
        String::new(),
    ];
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
            let uniform = group
                .iter()
                .map(|s| &s.version)
                .collect::<HashSet<_>>()
                .len()
                == 1;
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
                    "{weekday} {}., {}",
                    local.format("%d.%m"),
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
                chunks.push(std::mem::take(&mut current) + "\n");
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use chrono_tz::Europe::Vienna;

    fn make(
        cinema: &str,
        movie: &str,
        day: u32,
        hour: u32,
        minute: u32,
        version: &str,
        hall: &str,
        url: &str,
    ) -> Showing {
        Showing {
            cinema: cinema.into(),
            movie: movie.into(),
            start: Vienna
                .with_ymd_and_hms(2026, 7, day, hour, minute, 0)
                .unwrap()
                .with_timezone(&Utc),
            version: version.into(),
            hall: hall.into(),
            url: url.into(),
        }
    }

    #[test]
    fn groups_showings_under_movie_titles() {
        let showings = vec![
            make(
                "Megaplex PlusCity",
                "Die Odyssee",
                20,
                19,
                45,
                "OV - IMAX 2D",
                "",
                "https://www.megaplex.at/ticket/57419/539128",
            ),
            make(
                "Cineplexx Linz",
                "The Odyssey",
                21,
                20,
                15,
                "OV",
                "Saal 3",
                "https://cineplexx.at/film/die-odyssee",
            ),
            make(
                "Cineplexx Linz",
                "The Odyssey",
                20,
                19,
                0,
                "OV",
                "Saal 6",
                "https://cineplexx.at/film/die-odyssee",
            ),
        ];
        let msg = format_message(&showings, &HashMap::new());
        let lines: Vec<&str> = msg.split('\n').collect();
        assert_eq!(lines[0], "🎬 <b>Neue OV-Vorstellungen in Linz</b>");
        let pos = |needle: &str| lines.iter().position(|l| *l == needle).unwrap();
        assert!(pos("<b>Cineplexx Linz</b>") < pos("<b>Megaplex PlusCity</b>"));
        assert_eq!(msg.matches("<b>The Odyssey (OV)</b>").count(), 1);
        let monday =
            "• <a href=\"https://cineplexx.at/film/die-odyssee\">Saal 6 · Mo 20.07., 19:00</a>";
        let tuesday =
            "• <a href=\"https://cineplexx.at/film/die-odyssee\">Saal 3 · Di 21.07., 20:15</a>";
        assert!(msg.contains(monday) && msg.contains(tuesday));
        assert!(pos(monday) < pos(tuesday));
        assert!(msg.contains(
            "• <a href=\"https://www.megaplex.at/ticket/57419/539128\">Mo 20.07., 19:45</a>"
        ));
        assert!(!lines.iter().any(|l| l.starts_with("http")));
    }

    #[test]
    fn version_on_lines_when_versions_differ() {
        let showings = vec![
            make(
                "Cineplexx Linz",
                "F1",
                20,
                19,
                0,
                "OV",
                "Saal 6",
                "https://x/1",
            ),
            make(
                "Cineplexx Linz",
                "F1",
                22,
                18,
                30,
                "OmU",
                "Saal 1",
                "https://x/2",
            ),
        ];
        let msg = format_message(&showings, &HashMap::new());
        assert!(msg.contains("<b>F1</b>"));
        assert!(msg.contains("• <a href=\"https://x/1\">Saal 6 · Mo 20.07., 19:00 · OV</a>"));
        assert!(msg.contains("• <a href=\"https://x/2\">Saal 1 · Mi 22.07., 18:30 · OmU</a>"));
    }

    #[test]
    fn escapes_html() {
        let showings = vec![make(
            "Cineplexx Linz",
            "Fast & Furious <Final>",
            20,
            20,
            0,
            "OV",
            "",
            "https://x.at/film?a=1&b=2",
        )];
        let msg = format_message(&showings, &HashMap::new());
        assert!(msg.contains("<b>Fast &amp; Furious &lt;Final&gt; (OV)</b>"));
        assert!(msg.contains("href=\"https://x.at/film?a=1&amp;b=2\""));
        assert!(!msg.contains("Fast & Furious"));
    }

    #[test]
    fn appends_genre_and_runtime() {
        let showings = vec![make(
            "Cineplexx Linz",
            "The Odyssey",
            20,
            19,
            0,
            "OV",
            "Saal 6",
            "https://x",
        )];
        let movies = HashMap::from([(
            "Cineplexx Linz|The Odyssey".to_string(),
            MovieMeta {
                runtime_min: Some(180),
                genres: vec!["Abenteuer".into(), "Historie".into()],
                poster: None,
            },
        )]);
        let msg = format_message(&showings, &movies);
        assert!(msg.contains("<b>The Odyssey (OV)</b> — Abenteuer, Historie, 180 Min"));
    }

    #[test]
    fn meta_suffix_without_uniform_version() {
        let showings = vec![
            make(
                "Cineplexx Linz",
                "F1",
                20,
                19,
                0,
                "OV",
                "Saal 6",
                "https://x/1",
            ),
            make(
                "Cineplexx Linz",
                "F1",
                22,
                18,
                30,
                "OmU",
                "Saal 1",
                "https://x/2",
            ),
        ];
        let movies = HashMap::from([(
            "Cineplexx Linz|F1".to_string(),
            MovieMeta {
                runtime_min: Some(100),
                genres: vec!["Drama".into()],
                poster: None,
            },
        )]);
        let msg = format_message(&showings, &movies);
        assert!(msg.contains("<b>F1</b> — Drama, 100 Min"));
    }

    #[test]
    fn escapes_meta_genres() {
        let showings = vec![make(
            "Cineplexx Linz",
            "X",
            20,
            19,
            0,
            "OV",
            "",
            "https://x",
        )];
        let movies = HashMap::from([(
            "Cineplexx Linz|X".to_string(),
            MovieMeta {
                runtime_min: None,
                genres: vec!["Dra<ma> & Co".into()],
                poster: None,
            },
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
        let lines: Vec<String> = (0..200)
            .map(|i| format!("line {i} {}", "x".repeat(90)))
            .collect();
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

    #[test]
    fn chunk_preserves_newline_between_two_overlong_lines() {
        let text = format!("{}\n{}", "y".repeat(5000), "z".repeat(5000));
        let chunks = chunk_text(&text, MAX_LEN);
        assert!(chunks.iter().all(|c| c.chars().count() <= MAX_LEN));
        assert_eq!(chunks.concat(), text);
    }

    use axum::{extract::State as AxumState, routing::post, Json, Router};
    use std::sync::{Arc, Mutex};

    async fn spawn_capture_server() -> (String, Arc<Mutex<Vec<serde_json::Value>>>) {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let cap = captured.clone();
        let app = Router::new().route(
            "/botTOKEN/sendMessage",
            post(
                move |AxumState(()): AxumState<()>, Json(body): Json<serde_json::Value>| {
                    let cap = cap.clone();
                    async move {
                        cap.lock().unwrap().push(body);
                        "ok"
                    }
                },
            ),
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
        let joined: String = calls
            .iter()
            .map(|c| c["text"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(joined, text);
    }
}
