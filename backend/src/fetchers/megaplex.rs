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
        NaiveDate::from_ymd_opt(c[3].parse().ok()?, c[2].parse().ok()?, c[1].parse().ok()?)
    })
}

fn string_or_array(value: Option<&serde_json::Value>) -> Vec<String> {
    match value {
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        Some(serde_json::Value::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
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
        return Err(SourceError::msg(format!(
            "unexpected Megaplex film page: {url}"
        )));
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
            let combined = format!("{version} ");
            showings.push(Showing {
                cinema: MEGAPLEX_CINEMA_NAME.to_string(),
                movie: title.clone(),
                start: local.with_timezone(&Utc),
                version,
                hall: String::new(),
                url: full_url,
                features: crate::models::extract_features(&combined),
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
            .get_text(
                &format!("{MEGAPLEX_BASE}/kinoprogramm/linz/{day}/ov"),
                &headers,
            )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn megaplex_features_from_version() {
        let version = megaplex_version("OV - IMAX 2D").unwrap();
        let combined = format!("{version} ");
        assert_eq!(
            crate::models::extract_features(&combined),
            vec!["OV", "IMAX", "2D"]
        );
    }
}
