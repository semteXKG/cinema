use crate::web::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing;
use axum::Json;
use axum::Router;

pub fn telegram_webhook_router() -> Router<AppState> {
    Router::new().route(
        "/api/telegram/webhook/{secret}",
        routing::post(post_webhook),
    )
}

pub fn normalize_handle(handle: &str) -> String {
    handle.trim().trim_start_matches('@').to_lowercase()
}

async fn post_webhook(
    State(state): State<AppState>,
    Path(secret): Path<String>,
    Json(update): Json<serde_json::Value>,
) -> StatusCode {
    if state.telegram_webhook_secret.as_deref() != Some(secret.as_str()) {
        return StatusCode::UNAUTHORIZED;
    }
    let username = update
        .pointer("/message/from/username")
        .and_then(|v| v.as_str());
    let chat_id = update.pointer("/message/chat/id").and_then(|v| v.as_i64());
    let (Some(username), Some(chat_id)) = (username, chat_id) else {
        // Missing fields: still 200 so Telegram stops retrying this update.
        return StatusCode::OK;
    };
    let handle = normalize_handle(username);
    let updated = sqlx::query(
        "UPDATE notification_preferences
            SET telegram_chat_id = $1, updated_at = now()
          WHERE telegram_handle = $2 AND telegram_chat_id IS NULL",
    )
    .bind(chat_id.to_string())
    .bind(&handle)
    .execute(&state.pool)
    .await;
    match updated {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            tracing::error!("telegram webhook update failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::web::AppState;
    use axum::http::Request;
    use axum::http::StatusCode;
    use sqlx::PgPool;
    use std::path::PathBuf;
    use tower::ServiceExt;

    fn test_state(pool: PgPool) -> AppState {
        AppState {
            pool,
            data_dir: PathBuf::new(),
            static_dir: PathBuf::from("/nonexistent"),
            base_url: "http://localhost:8080".into(),
            fake_login: false,
            smtp_config: None,
            google_oauth: None,
            github_oauth: None,
            telegram_webhook_secret: Some("supersecret".into()),
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn webhook_verifies_handle_and_stores_chat_id(pool: PgPool) {
        let uid = crate::db::find_or_create_user(&pool, "email", "a@b.com", "a@b.com")
            .await
            .unwrap();
        crate::notification::db::upsert_preferences(
            &pool,
            uid,
            crate::notification::db::PreferenceUpdate {
                telegram_handle: Some("myhandle".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let app = crate::web::router(test_state(pool.clone()));
        // matching handle with wrong chat_id previously None
        let resp = app
            .oneshot(
                Request::post("/api/telegram/webhook/supersecret")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"update_id":1,"message":{"message_id":1,"from":{"id":99,"username":"MyHandle"},"chat":{"id":12345}}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let prefs = crate::notification::db::get_preferences(&pool, uid)
            .await
            .unwrap();
        assert_eq!(prefs.telegram_chat_id.as_deref(), Some("12345"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn webhook_rejects_wrong_secret(pool: PgPool) {
        let app = crate::web::router(test_state(pool));
        let resp = app
            .oneshot(
                Request::post("/api/telegram/webhook/wrong")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
