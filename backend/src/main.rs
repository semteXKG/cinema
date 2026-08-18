mod auth;
mod checker;
mod config;
mod db;
mod fetchers;
mod ics;
mod models;
mod notification;
mod notify;
mod web;

use checker::{CineplexxFetcher, Fetcher, MegaplexFetcher};
use chrono::Utc;
use config::Config;
use fetchers::HttpClient;
use notify::{Notifier, TelegramNotifier};
use sqlx::PgPool;
use std::future::Future;
use std::net::SocketAddr;
use std::time::Duration;

pub async fn scheduler_loop<F, Fut>(interval: Duration, run: F)
where
    F: Fn() -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await; // first tick fires immediately
        if let Err(e) = run().await {
            tracing::error!("check run failed: {e:#}");
        }
    }
}

async fn run_default_check(pool: &PgPool, config: &Config) -> anyhow::Result<()> {
    let http = HttpClient::new(Duration::from_millis(500));
    let cineplexx = CineplexxFetcher;
    let megaplex = MegaplexFetcher;
    let mut active: Vec<(&str, &dyn Fetcher)> = Vec::new();
    for source in &config.sources {
        match source.as_str() {
            "cineplexx" => active.push(("cineplexx", &cineplexx)),
            "megaplex" => active.push(("megaplex", &megaplex)),
            other => tracing::warn!("unknown source {other}"),
        }
    }
    let notifier = match (&config.telegram_token, &config.telegram_chat_id) {
        (Some(token), Some(chat_id)) => Some(TelegramNotifier::new(token, chat_id)),
        _ => None,
    };
    let email_sender: Option<crate::notification::send::EmailNotifier> =
        match (&config.smtp_host, &config.smtp_password, &config.smtp_from) {
            (Some(host), Some(password), Some(from)) => {
                let smtp = web::SmtpConfig {
                    host: host.clone(),
                    port: config.smtp_port,
                    username: config.smtp_username.clone(),
                    password: password.clone(),
                    from: from.clone(),
                };
                let from = config
                    .notification_email_from
                    .clone()
                    .unwrap_or_else(|| default_email_from(&config.base_url));
                match crate::auth::build_mailer(&smtp) {
                    Ok(mailer) => Some(crate::notification::send::EmailNotifier::new(mailer, from)),
                    Err(status) => {
                        tracing::warn!("smtp mailer build failed: {status}");
                        None
                    }
                }
            }
            _ => None,
        };
    let telegram_dm: Option<crate::notify::TelegramDmNotifier> = config
        .telegram_token
        .as_ref()
        .map(|t| crate::notify::TelegramDmNotifier::new(t));
    let ctx = checker::CheckCtx {
        pool,
        http: &http,
        config,
        notifier: notifier.as_ref().map(|n| n as &dyn Notifier),
        fetchers: active,
        email: email_sender
            .as_ref()
            .map(|e| e as &dyn crate::notification::batch::EmailSender),
        telegram: telegram_dm
            .as_ref()
            .map(|t| t as &dyn crate::notification::batch::TelegramSender),
    };
    let result = checker::run_check(&ctx, Utc::now()).await?;
    tracing::info!(
        new = result.new_showings,
        total = result.total_showings,
        "check finished"
    );
    Ok(())
}

fn default_email_from(base_url: &str) -> String {
    let host = base_url
        .strip_prefix("http://")
        .or_else(|| base_url.strip_prefix("https://"))
        .unwrap_or(base_url)
        .split(['/', ':'])
        .next()
        .unwrap_or(base_url);
    format!("showings@{host}")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ov_watcher=info,tower_http=info".into()),
        )
        .init();
    let config = Config::from_env()?;
    let pool = PgPool::connect(&config.database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    if let Err(e) = db::prune_expired_sessions(&pool).await {
        tracing::warn!("failed to prune expired sessions: {e}");
    }

    {
        let pool = pool.clone();
        let config = config.clone();
        tokio::spawn(async move {
            scheduler_loop(config.check_interval, move || {
                let pool = pool.clone();
                let config = config.clone();
                async move { run_default_check(&pool, &config).await }
            })
            .await;
        });
    }

    {
        let pool = pool.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
            loop {
                interval.tick().await;
                if let Err(e) = db::prune_expired_sessions(&pool).await {
                    tracing::error!("session cleanup failed: {e}");
                }
            }
        });
    }

    let smtp_config = match (&config.smtp_host, &config.smtp_password, &config.smtp_from) {
        (Some(host), Some(password), Some(from)) => Some(web::SmtpConfig {
            host: host.clone(),
            port: config.smtp_port,
            username: config.smtp_username.clone(),
            password: password.clone(),
            from: from.clone(),
        }),
        _ => None,
    };
    let state = web::AppState {
        pool: pool.clone(),
        data_dir: config.data_dir.clone(),
        static_dir: config.static_dir.clone(),
        base_url: config.base_url.clone(),
        fake_login: config.fake_login,
        smtp_config,
        google_oauth: match (&config.google_client_id, &config.google_client_secret) {
            (Some(id), Some(secret)) => Some(web::OAuthConfig {
                client_id: id.clone(),
                client_secret: secret.clone(),
            }),
            _ => None,
        },
        github_oauth: match (&config.github_client_id, &config.github_client_secret) {
            (Some(id), Some(secret)) => Some(web::OAuthConfig {
                client_id: id.clone(),
                client_secret: secret.clone(),
            }),
            _ => None,
        },
        telegram_webhook_secret: config.telegram_webhook_secret.clone(),
    };
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!("starting web server on port {}", config.port);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, web::router(state)).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn default_email_from_derives_host() {
        assert_eq!(
            super::default_email_from("https://cinema.k-labs.app"),
            "showings@cinema.k-labs.app"
        );
        assert_eq!(
            super::default_email_from("http://localhost:8080"),
            "showings@localhost"
        );
        assert_eq!(
            super::default_email_from("https://k-labs.app/"),
            "showings@k-labs.app"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn scheduler_runs_immediately_and_repeatedly() {
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let handle = tokio::spawn(super::scheduler_loop(
            Duration::from_secs(3600),
            move || {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            },
        ));
        tokio::task::yield_now().await; // let the spawned loop register its interval timer
        tokio::time::advance(Duration::from_secs(7200)).await;
        for _ in 0..10 {
            tokio::task::yield_now().await; // let the spawned loop consume its ticks
        }
        handle.abort();
        assert_eq!(count.load(Ordering::SeqCst), 3); // immediate + 2 intervals
    }

    #[tokio::test(start_paused = true)]
    async fn scheduler_survives_failures() {
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let handle = tokio::spawn(super::scheduler_loop(Duration::from_secs(60), move || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err(anyhow::anyhow!("boom"))
            }
        }));
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(120)).await;
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        handle.abort();
        assert_eq!(count.load(Ordering::SeqCst), 3);
    }
}
