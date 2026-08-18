use crate::auth::AuthUser;
use crate::web::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing;
use axum::Router;
use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceUpdateRequest {
    pub email_frequency: Option<String>,
    pub telegram_frequency: Option<String>,
    pub telegram_handle: Option<String>,
    pub digest_anchor: Option<DateTime<Utc>>,
    pub digest_hour: Option<i32>,
}

impl From<PreferenceUpdateRequest> for crate::notification::db::PreferenceUpdate {
    fn from(req: PreferenceUpdateRequest) -> Self {
        Self {
            email_frequency: req.email_frequency,
            telegram_frequency: req.telegram_frequency,
            telegram_handle: req.telegram_handle,
            digest_anchor: req.digest_anchor,
            digest_hour: req.digest_hour,
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesResponse {
    pub email_frequency: String,
    pub telegram_frequency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telegram_handle: Option<String>,
    pub telegram_verified: bool,
    pub digest_anchor: DateTime<Utc>,
    pub digest_hour: i32,
}

impl From<crate::notification::db::NotificationPreferences> for PreferencesResponse {
    fn from(p: crate::notification::db::NotificationPreferences) -> Self {
        PreferencesResponse {
            email_frequency: p.email_frequency,
            telegram_frequency: p.telegram_frequency,
            telegram_handle: p.telegram_handle,
            telegram_verified: p.telegram_chat_id.is_some(),
            digest_anchor: p.digest_anchor,
            digest_hour: p.digest_hour,
        }
    }
}

pub fn preferences_router() -> Router<AppState> {
    Router::new()
        .route("/api/preferences", routing::get(get_preferences))
        .route("/api/preferences", routing::put(put_preferences))
        .route(
            "/api/preferences/telegram",
            routing::delete(delete_telegram),
        )
}

async fn get_preferences(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<PreferencesResponse>, StatusCode> {
    let prefs = crate::notification::db::get_preferences(&state.pool, auth.user_id)
        .await
        .map_err(|e| {
            tracing::error!("get_preferences failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(prefs.into()))
}

async fn put_preferences(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<PreferenceUpdateRequest>,
) -> Result<Json<PreferencesResponse>, StatusCode> {
    let dto = crate::notification::db::PreferenceUpdate::from(body);
    validate_update(&dto)?;
    let changed_digest = dto.digest_anchor.is_some() || dto.digest_hour.is_some();
    let rollover_email = dto.email_frequency.is_some() || changed_digest;
    let rollover_telegram = dto.telegram_frequency.is_some() || changed_digest;
    let updated = crate::notification::db::upsert_preferences(&state.pool, auth.user_id, dto)
        .await
        .map_err(|e| {
            tracing::error!("upsert_preferences failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if rollover_email {
        rollover_batch(&state, auth.user_id, "email").await;
    }
    if rollover_telegram {
        rollover_batch(&state, auth.user_id, "telegram").await;
    }
    Ok(Json(updated.into()))
}

async fn rollover_batch(state: &AppState, user_id: i64, layer: &str) {
    if let Err(e) = crate::notification::db::delete_open_batch(&state.pool, user_id, layer).await {
        tracing::warn!("delete open {layer} batch failed: {e}");
    }
}

async fn delete_telegram(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<PreferencesResponse>, StatusCode> {
    crate::notification::db::upsert_preferences(
        &state.pool,
        auth.user_id,
        crate::notification::db::PreferenceUpdate {
            telegram_frequency: Some("never".into()),
            telegram_handle: Some(String::new()),
            ..Default::default()
        },
    )
    .await
    .map_err(|e| {
        tracing::error!("delete telegram prefs failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    rollover_batch(&state, auth.user_id, "telegram").await;
    let prefs = crate::notification::db::get_preferences(&state.pool, auth.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(prefs.into()))
}

fn is_valid_frequency(v: &str) -> bool {
    v == "never" || v == "immediately" || matches!(v.parse::<i32>(), Ok(d) if (1..=7).contains(&d))
}

fn validate_update(dto: &crate::notification::db::PreferenceUpdate) -> Result<(), StatusCode> {
    if let Some(f) = &dto.email_frequency {
        if !is_valid_frequency(f) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    if let Some(f) = &dto.telegram_frequency {
        if !is_valid_frequency(f) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    if let Some(h) = dto.digest_hour {
        if !(0..=23).contains(&h) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::web::AppState;
    use axum::body::to_bytes;
    use axum::http::Request;
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
        }
    }

    async fn make_session(pool: &PgPool, user_id: i64) -> String {
        let token = crate::auth::new_token();
        let expires = chrono::Utc::now() + chrono::Duration::days(30);
        crate::db::create_session(pool, user_id, &token, expires)
            .await
            .unwrap();
        token
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_preferences_defaults(pool: PgPool) {
        let uid = crate::db::find_or_create_user(&pool, "email", "a@b.com", "a@b.com")
            .await
            .unwrap();
        let token = make_session(&pool, uid).await;
        let state = test_state(pool);
        let app = crate::web::router(state);
        let resp = app
            .oneshot(
                Request::get("/api/preferences")
                    .header("Cookie", format!("ov_session={token}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["emailFrequency"], "never");
        assert_eq!(json["telegramFrequency"], "never");
        assert_eq!(json["telegramVerified"], false);
        assert_eq!(json["telegramHandle"], serde_json::Value::Null);
        assert!(json["digestAnchor"].is_string());
        assert_eq!(json["digestHour"], 9);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_preferences_unauthenticated_401(pool: PgPool) {
        let state = test_state(pool);
        let app = crate::web::router(state);
        let resp = app
            .oneshot(
                Request::get("/api/preferences")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 401);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn put_preferences_updates_and_returns_values(pool: PgPool) {
        let uid = crate::db::find_or_create_user(&pool, "email", "a@b.com", "a@b.com")
            .await
            .unwrap();
        let token = make_session(&pool, uid).await;
        let state = test_state(pool);
        let app = crate::web::router(state);
        let resp = app
            .clone()
            .oneshot(
                Request::put("/api/preferences")
                    .header("Cookie", format!("ov_session={token}"))
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"emailFrequency":"immediately","telegramFrequency":"3","telegramHandle":"@MyHandle","digestHour":10}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["emailFrequency"], "immediately");
        assert_eq!(json["telegramFrequency"], "3");
        assert_eq!(json["telegramHandle"], "myhandle");
        assert_eq!(json["telegramVerified"], false);
        assert_eq!(json["digestHour"], 10);

        // GET reflects the saved state
        let resp = app
            .oneshot(
                Request::get("/api/preferences")
                    .header("Cookie", format!("ov_session={token}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["emailFrequency"], "immediately");
        assert_eq!(json["telegramFrequency"], "3");
        assert_eq!(json["telegramHandle"], "myhandle");
        assert_eq!(json["digestHour"], 10);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn put_preferences_rejects_invalid_frequency(pool: PgPool) {
        let uid = crate::db::find_or_create_user(&pool, "email", "a@b.com", "a@b.com")
            .await
            .unwrap();
        let token = make_session(&pool, uid).await;
        let state = test_state(pool);
        let app = crate::web::router(state);

        for bad in ["sometimes", "0", "8", ""] {
            let resp = app
                .clone()
                .oneshot(
                    Request::put("/api/preferences")
                        .header("Cookie", format!("ov_session={token}"))
                        .header("Content-Type", "application/json")
                        .body(axum::body::Body::from(format!(
                            r#"{{"emailFrequency":"{bad}"}}"#
                        )))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), 400, "expected 400 for {bad:?}");
        }
        let resp = app
            .oneshot(
                Request::put("/api/preferences")
                    .header("Cookie", format!("ov_session={token}"))
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(r#"{"telegramFrequency":"never"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn put_preferences_rejects_invalid_digest_hour(pool: PgPool) {
        let uid = crate::db::find_or_create_user(&pool, "email", "a@b.com", "a@b.com")
            .await
            .unwrap();
        let token = make_session(&pool, uid).await;
        let state = test_state(pool);
        let app = crate::web::router(state);
        let resp = app
            .oneshot(
                Request::put("/api/preferences")
                    .header("Cookie", format!("ov_session={token}"))
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(r#"{"digestHour":25}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 400);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn put_preferences_rolls_over_open_batches(pool: PgPool) {
        let uid = crate::db::find_or_create_user(&pool, "email", "a@b.com", "a@b.com")
            .await
            .unwrap();
        crate::notification::db::get_or_create_open_batch(&pool, uid, "email")
            .await
            .unwrap();
        crate::notification::db::get_or_create_open_batch(&pool, uid, "telegram")
            .await
            .unwrap();

        let token = make_session(&pool, uid).await;
        let state = test_state(pool.clone());
        let app = crate::web::router(state);
        let resp = app
            .oneshot(
                Request::put("/api/preferences")
                    .header("Cookie", format!("ov_session={token}"))
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"emailFrequency":"immediately","telegramFrequency":"2"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        let email_count: (i64,) =
            sqlx::query_as("SELECT count(*) FROM notification_batch WHERE user_id = $1 AND layer = 'email' AND status = 'pending'")
                .bind(uid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(email_count.0, 0, "open email batch should be rolled over");
        let telegram_count: (i64,) =
            sqlx::query_as("SELECT count(*) FROM notification_batch WHERE user_id = $1 AND layer = 'telegram' AND status = 'pending'")
                .bind(uid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            telegram_count.0, 0,
            "open telegram batch should be rolled over"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn delete_telegram_clears_handle_and_sets_never(pool: PgPool) {
        let uid = crate::db::find_or_create_user(&pool, "email", "a@b.com", "a@b.com")
            .await
            .unwrap();
        crate::notification::db::upsert_preferences(
            &pool,
            uid,
            crate::notification::db::PreferenceUpdate {
                telegram_frequency: Some("immediately".into()),
                telegram_handle: Some("myhandle".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        crate::notification::db::get_or_create_open_batch(&pool, uid, "telegram")
            .await
            .unwrap();

        let token = make_session(&pool, uid).await;
        let state = test_state(pool);
        let app = crate::web::router(state);
        let resp = app
            .clone()
            .oneshot(
                Request::delete("/api/preferences/telegram")
                    .header("Cookie", format!("ov_session={token}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["telegramFrequency"], "never");
        assert_eq!(json["telegramHandle"], serde_json::Value::Null);
        assert_eq!(json["telegramVerified"], false);

        // GET reflects cleared state
        let resp = app
            .oneshot(
                Request::get("/api/preferences")
                    .header("Cookie", format!("ov_session={token}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["telegramFrequency"], "never");
        assert_eq!(json["telegramHandle"], serde_json::Value::Null);
    }
}
