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
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub smtp_from: Option<String>,
    pub base_url: String,
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
    pub apple_client_id: Option<String>,
    pub apple_team_id: Option<String>,
    pub apple_key_id: Option<String>,
    pub apple_private_key: Option<String>,
    pub github_client_id: Option<String>,
    pub github_client_secret: Option<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        // Treat empty-string values as unset. Deployment configs (Helm, etc.)
        // render unconfigured optional vars as "" rather than omitting them;
        // those must not be seen as configured values or parse errors.
        let get = |key: &str| get(key).filter(|v| !v.trim().is_empty());
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
        let smtp_port: u16 = get("SMTP_PORT")
            .unwrap_or_else(|| "587".into())
            .parse()
            .map_err(|_| anyhow::anyhow!("SMTP_PORT must be a number"))?;
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
            smtp_host: get("SMTP_HOST"),
            smtp_port,
            smtp_username: get("SMTP_USERNAME"),
            smtp_password: get("SMTP_PASSWORD"),
            smtp_from: get("SMTP_FROM"),
            base_url: get("BASE_URL").unwrap_or_else(|| "https://cinema.k-labs.app".into()),
            google_client_id: get("GOOGLE_CLIENT_ID"),
            google_client_secret: get("GOOGLE_CLIENT_SECRET"),
            apple_client_id: get("APPLE_CLIENT_ID"),
            apple_team_id: get("APPLE_TEAM_ID"),
            apple_key_id: get("APPLE_KEY_ID"),
            apple_private_key: get("APPLE_PRIVATE_KEY"),
            github_client_id: get("GITHUB_CLIENT_ID"),
            github_client_secret: get("GITHUB_CLIENT_SECRET"),
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
        assert_eq!(cfg.smtp_port, 587);
        assert_eq!(cfg.smtp_host, None);
        assert_eq!(cfg.smtp_from, None);
        assert_eq!(cfg.base_url, "https://cinema.k-labs.app");
        assert_eq!(cfg.google_client_id, None);
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
            ("SMTP_HOST", "smtp.example.com"),
            ("SMTP_PORT", "465"),
            ("SMTP_FROM", "OV-Kino <noreply@k-labs.app>"),
            ("BASE_URL", "http://localhost:8080"),
            ("GOOGLE_CLIENT_ID", "gcid"),
            ("GOOGLE_CLIENT_SECRET", "gcs"),
            ("APPLE_CLIENT_ID", "acid"),
            ("APPLE_TEAM_ID", "atid"),
            ("APPLE_KEY_ID", "akid"),
            ("APPLE_PRIVATE_KEY", "apk"),
            ("GITHUB_CLIENT_ID", "ghcid"),
            ("GITHUB_CLIENT_SECRET", "ghcs"),
        ]))
        .unwrap();
        assert_eq!(cfg.telegram_token.as_deref(), Some("T"));
        assert_eq!(cfg.telegram_chat_id.as_deref(), Some("@ov_linz"));
        assert_eq!(cfg.sources, vec!["cineplexx", "megaplex"]);
        assert_eq!(cfg.check_interval, Duration::from_secs(5400));
        assert_eq!(cfg.data_dir, PathBuf::from("/data"));
        assert_eq!(cfg.port, 9090);
        assert_eq!(cfg.smtp_host.as_deref(), Some("smtp.example.com"));
        assert_eq!(cfg.smtp_port, 465);
        assert_eq!(
            cfg.smtp_from.as_deref(),
            Some("OV-Kino <noreply@k-labs.app>")
        );
        assert_eq!(cfg.base_url, "http://localhost:8080");
        assert_eq!(cfg.google_client_id.as_deref(), Some("gcid"));
        assert_eq!(cfg.google_client_secret.as_deref(), Some("gcs"));
        assert_eq!(cfg.apple_client_id.as_deref(), Some("acid"));
        assert_eq!(cfg.apple_team_id.as_deref(), Some("atid"));
        assert_eq!(cfg.apple_key_id.as_deref(), Some("akid"));
        assert_eq!(cfg.apple_private_key.as_deref(), Some("apk"));
        assert_eq!(cfg.github_client_id.as_deref(), Some("ghcid"));
        assert_eq!(cfg.github_client_secret.as_deref(), Some("ghcs"));
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
    fn invalid_smtp_port_is_an_error() {
        let cfg = Config::from_lookup(env_of(&[
            ("DATABASE_URL", "postgres://x"),
            ("SMTP_PORT", "abc"),
        ]));
        assert!(cfg.is_err());
    }

    #[test]
    fn empty_string_env_values_are_treated_as_unset() {
        // Helm renders optional env vars as "" when unset; the parser must
        // treat those as absent, not as configured values or parse errors.
        let cfg = Config::from_lookup(env_of(&[
            ("DATABASE_URL", "postgres://x"),
            ("SMTP_HOST", ""),
            ("SMTP_PORT", ""),
            ("SMTP_USERNAME", ""),
            ("SMTP_PASSWORD", ""),
            ("SMTP_FROM", ""),
            ("GOOGLE_CLIENT_ID", ""),
            ("GOOGLE_CLIENT_SECRET", ""),
            ("APPLE_CLIENT_ID", ""),
            ("APPLE_TEAM_ID", ""),
            ("APPLE_KEY_ID", ""),
            ("APPLE_PRIVATE_KEY", ""),
            ("GITHUB_CLIENT_ID", ""),
            ("GITHUB_CLIENT_SECRET", ""),
            ("TELEGRAM_BOT_TOKEN", ""),
            ("TELEGRAM_CHAT_ID", ""),
        ]))
        .unwrap();
        assert_eq!(cfg.smtp_port, 587);
        assert_eq!(cfg.smtp_host, None);
        assert_eq!(cfg.smtp_username, None);
        assert_eq!(cfg.smtp_password, None);
        assert_eq!(cfg.smtp_from, None);
        assert_eq!(cfg.google_client_id, None);
        assert_eq!(cfg.apple_team_id, None);
        assert_eq!(cfg.github_client_secret, None);
        assert_eq!(cfg.telegram_token, None);
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
