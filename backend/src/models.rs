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
