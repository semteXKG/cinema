use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub telegram_token: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub sources: Vec<String>,
    pub check_interval: Duration,
    pub data_dir: PathBuf,
    pub port: u16,
    pub database_url: String,
    pub static_dir: PathBuf,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        let database_url =
            get("DATABASE_URL").ok_or_else(|| anyhow::anyhow!("DATABASE_URL is required"))?;
        let sources = get("SOURCES")
            .unwrap_or_else(|| "cineplexx,megaplex".into())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let hours: f64 = get("CHECK_INTERVAL_HOURS")
            .unwrap_or_else(|| "3".into())
            .parse()
            .map_err(|_| anyhow::anyhow!("CHECK_INTERVAL_HOURS must be a number"))?;
        if !hours.is_finite() || hours < 0.0 {
            return Err(anyhow::anyhow!(
                "CHECK_INTERVAL_HOURS must be a non-negative number"
            ));
        }
        let port: u16 = get("PORT")
            .unwrap_or_else(|| "8080".into())
            .parse()
            .map_err(|_| anyhow::anyhow!("PORT must be a number"))?;
        Ok(Config {
            telegram_token: get("TELEGRAM_BOT_TOKEN"),
            telegram_chat_id: get("TELEGRAM_CHAT_ID"),
            sources,
            check_interval: Duration::from_secs_f64(hours * 3600.0),
            data_dir: PathBuf::from(get("DATA_DIR").unwrap_or_else(|| "./data".into())),
            port,
            database_url,
            static_dir: PathBuf::from(
                get("STATIC_DIR").unwrap_or_else(|| "./frontend/dist".into()),
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key| map.get(key).cloned()
    }

    #[test]
    fn defaults_when_only_database_url_set() {
        let cfg = Config::from_lookup(env_of(&[("DATABASE_URL", "postgres://x")])).unwrap();
        assert_eq!(cfg.sources, vec!["cineplexx", "megaplex"]);
        assert_eq!(cfg.check_interval, Duration::from_secs(3 * 3600));
        assert_eq!(cfg.data_dir, PathBuf::from("./data"));
        assert_eq!(cfg.static_dir, PathBuf::from("./frontend/dist"));
        assert_eq!(cfg.port, 8080);
        assert_eq!(cfg.telegram_token, None);
    }

    #[test]
    fn parses_all_overrides() {
        let cfg = Config::from_lookup(env_of(&[
            ("DATABASE_URL", "postgres://x"),
            ("TELEGRAM_BOT_TOKEN", "T"),
            ("TELEGRAM_CHAT_ID", "@ov_linz"),
            ("SOURCES", "cineplexx, megaplex,"),
            ("CHECK_INTERVAL_HOURS", "1.5"),
            ("DATA_DIR", "/data"),
            ("PORT", "9090"),
            ("STATIC_DIR", "/srv/static"),
        ]))
        .unwrap();
        assert_eq!(cfg.telegram_token.as_deref(), Some("T"));
        assert_eq!(cfg.telegram_chat_id.as_deref(), Some("@ov_linz"));
        assert_eq!(cfg.sources, vec!["cineplexx", "megaplex"]);
        assert_eq!(cfg.check_interval, Duration::from_secs(5400));
        assert_eq!(cfg.data_dir, PathBuf::from("/data"));
        assert_eq!(cfg.port, 9090);
    }

    #[test]
    fn missing_database_url_is_an_error() {
        assert!(Config::from_lookup(env_of(&[])).is_err());
    }

    #[test]
    fn invalid_port_is_an_error() {
        let cfg = Config::from_lookup(env_of(&[("DATABASE_URL", "postgres://x"), ("PORT", "abc")]));
        assert!(cfg.is_err());
    }

    #[test]
    fn non_finite_or_negative_check_interval_is_an_error() {
        for bad in ["-1", "nan", "inf", "abc"] {
            let cfg = Config::from_lookup(env_of(&[
                ("DATABASE_URL", "postgres://x"),
                ("CHECK_INTERVAL_HOURS", bad),
            ]));
            assert!(
                cfg.is_err(),
                "expected error for CHECK_INTERVAL_HOURS={bad}"
            );
        }
    }
}
