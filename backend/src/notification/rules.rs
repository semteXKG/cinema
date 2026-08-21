use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub cinema_id: Option<i64>,
    pub features: Vec<String>,
    pub title_substring: Option<String>,
    pub frequency: String,
    pub channels: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MatchableShowing {
    pub showing_id: i64,
    pub cinema_id: i64,
    pub features: Vec<String>,
    pub title: String,
}

pub fn matches(rule: &Rule, s: &MatchableShowing) -> bool {
    if let Some(cid) = rule.cinema_id {
        if cid != s.cinema_id {
            return false;
        }
    }
    if !rule.features.is_empty() {
        let need: HashSet<&str> = rule.features.iter().map(|s| s.as_str()).collect();
        let have: HashSet<&str> = s.features.iter().map(|s| s.as_str()).collect();
        if need.intersection(&have).next().is_none() {
            return false;
        }
    }
    if let Some(t) = rule.title_substring.as_deref() {
        let t = t.trim();
        if !t.is_empty() && !s.title.to_lowercase().contains(&t.to_lowercase()) {
            return false;
        }
    }
    true
}

#[allow(clippy::manual_find)]
pub fn first_match<'a>(rules: &'a [Rule], s: &MatchableShowing) -> Option<&'a Rule> {
    for r in rules {
        if matches(r, s) {
            return Some(r);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(cinema_id: Option<i64>, features: &[&str], title: Option<&str>, freq: &str) -> Rule {
        Rule {
            cinema_id,
            features: features.iter().map(|s| s.to_string()).collect(),
            title_substring: title.map(|s| s.to_string()),
            frequency: freq.to_string(),
            channels: vec!["email".into()],
        }
    }

    fn showing(cinema_id: i64, features: &[&str], title: &str) -> MatchableShowing {
        MatchableShowing {
            showing_id: 1,
            cinema_id,
            features: features.iter().map(|s| s.to_string()).collect(),
            title: title.to_string(),
        }
    }

    #[test]
    fn empty_features_matches_anything() {
        let r = rule(None, &[], None, "3");
        assert!(matches(&r, &showing(1, &["IMAX"], "X")));
        assert!(matches(&r, &showing(2, &[], "Y")));
    }

    #[test]
    fn any_of_overlap_matches() {
        let r = rule(None, &["IMAX", "Atmos"], None, "immediately");
        assert!(matches(&r, &showing(1, &["OV", "Atmos"], "X")));
        assert!(matches(&r, &showing(1, &["IMAX", "2D"], "X")));
        assert!(!matches(&r, &showing(1, &["OV", "2D"], "X")));
    }

    #[test]
    fn cinema_specific_and_any() {
        let r = rule(Some(7), &[], None, "immediately");
        assert!(matches(&r, &showing(7, &[], "X")));
        assert!(!matches(&r, &showing(8, &[], "X")));
    }

    #[test]
    fn title_substring_case_insensitive() {
        let r = rule(None, &[], Some("odyssey"), "immediately");
        assert!(matches(&r, &showing(1, &[], "The Odyssey")));
        assert!(!matches(&r, &showing(1, &[], "F1")));
    }

    #[test]
    fn title_substring_trimmed_empty_is_any() {
        let r = rule(None, &[], Some("   "), "3");
        assert!(matches(&r, &showing(1, &[], "Anything")));
    }

    #[test]
    fn first_match_wins_in_order() {
        let rules = vec![
            rule(Some(7), &["IMAX"], None, "immediately"),
            rule(None, &[], None, "3"),
        ];
        assert_eq!(
            first_match(&rules, &showing(7, &["IMAX"], "X")).map(|r| r.frequency.as_str()),
            Some("immediately")
        );
        assert_eq!(
            first_match(&rules, &showing(8, &["OV"], "Y")).map(|r| r.frequency.as_str()),
            Some("3")
        );
    }

    #[test]
    fn no_match_returns_none() {
        let rules = vec![rule(Some(7), &[], None, "immediately")];
        assert_eq!(first_match(&rules, &showing(9, &[], "X")), None);
    }
}
