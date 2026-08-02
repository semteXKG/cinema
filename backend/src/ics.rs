use chrono::{DateTime, Utc};
use sha1::{Digest, Sha1};

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
    let key = format!(
        "{}|{}|{}",
        s.cinema,
        s.movie,
        crate::models::vienna_iso(s.start)
    );
    format!("{:x}@ov-kino-linz", Sha1::digest(key.as_bytes()))
}

pub fn render_ics(showings: &[IcsShowing], now: DateTime<Utc>) -> String {
    let stamp = fmt_utc(now);
    let mut lines: Vec<String> = CAL_HEADER.iter().map(|s| s.to_string()).collect();
    for s in showings {
        let duration = s
            .runtime_min
            .filter(|&r| r > 0)
            .unwrap_or(DEFAULT_DURATION_MIN as i32) as i64;
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
        let a = render_ics(
            std::slice::from_ref(&s),
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        );
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
