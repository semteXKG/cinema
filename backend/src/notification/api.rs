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
    pub email_enabled: Option<bool>,
    pub telegram_enabled: Option<bool>,
    pub telegram_handle: Option<String>,
    pub digest_anchor: Option<DateTime<Utc>>,
    pub digest_hour: Option<i32>,
}

impl From<PreferenceUpdateRequest> for crate::notification::db::PreferenceUpdate {
    fn from(req: PreferenceUpdateRequest) -> Self {
        Self {
            email_enabled: req.email_enabled,
            telegram_enabled: req.telegram_enabled,
            telegram_handle: req.telegram_handle,
            digest_anchor: req.digest_anchor,
            digest_hour: req.digest_hour,
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesResponse {
    pub email_enabled: bool,
    pub telegram_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telegram_handle: Option<String>,
    pub telegram_verified: bool,
    pub digest_anchor: DateTime<Utc>,
    pub digest_hour: i32,
}

impl From<crate::notification::db::NotificationPreferences> for PreferencesResponse {
    fn from(p: crate::notification::db::NotificationPreferences) -> Self {
        PreferencesResponse {
            email_enabled: p.email_enabled,
            telegram_enabled: p.telegram_enabled,
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
    let rollover_email = dto.email_enabled.is_some() || changed_digest;
    let rollover_telegram = dto.telegram_enabled.is_some() || changed_digest;
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
            telegram_enabled: Some(false),
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

fn validate_update(dto: &crate::notification::db::PreferenceUpdate) -> Result<(), StatusCode> {
    if let Some(h) = dto.digest_hour {
        if !(0..=23).contains(&h) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    Ok(())
}

const FEATURE_VOCABULARY: &[&str] = &[
    "OV",
    "OmU",
    "OmdU",
    "2D",
    "3D",
    "IMAX",
    "Atmos",
    "DolbyCinema",
    "4DX",
];
const MAX_RULES: usize = 32;
const MAX_TITLE_LEN: usize = 200;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleRequest {
    pub cinema_id: Option<i64>,
    pub features: Vec<String>,
    pub title_substring: Option<String>,
    pub frequency: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleResponse {
    pub id: i64,
    pub position: i32,
    pub cinema_id: Option<i64>,
    pub cinema_name: Option<String>,
    pub features: Vec<String>,
    pub title_substring: Option<String>,
    pub frequency: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RulesResponse {
    pub rules: Vec<RuleResponse>,
    pub cinemas: Vec<CinemaDto>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CinemaDto {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RulesPutRequest {
    pub rules: Vec<RuleRequest>,
}

pub fn rules_router() -> Router<AppState> {
    Router::new()
        .route("/api/preferences/rules", routing::get(get_rules))
        .route("/api/preferences/rules", routing::put(put_rules))
}

async fn get_rules(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<RulesResponse>, StatusCode> {
    let rules = crate::notification::db::list_rules(&state.pool, auth.user_id)
        .await
        .map_err(|e| {
            tracing::error!("list_rules failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let cinemas = crate::notification::db::list_cinemas(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("list_cinemas failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let cinema_map: std::collections::HashMap<i64, String> = cinemas.iter().cloned().collect();
    let rules = rules
        .into_iter()
        .map(|r| RuleResponse {
            id: r.id,
            position: r.position,
            cinema_name: r.cinema_id.and_then(|id| cinema_map.get(&id).cloned()),
            cinema_id: r.cinema_id,
            features: r.features,
            title_substring: r.title_substring,
            frequency: r.frequency,
        })
        .collect();
    let cinemas = cinemas
        .into_iter()
        .map(|(id, name)| CinemaDto { id, name })
        .collect();
    Ok(Json(RulesResponse { rules, cinemas }))
}

async fn put_rules(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<RulesPutRequest>,
) -> Result<Json<RulesResponse>, StatusCode> {
    let cinemas = crate::notification::db::list_cinemas(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("list_cinemas failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let cinema_map: std::collections::HashMap<i64, String> = cinemas.iter().cloned().collect();
    validate_rules(&body.rules, &cinema_map)?;
    let input: Vec<crate::notification::db::RuleInput> = body
        .rules
        .into_iter()
        .map(|r| crate::notification::db::RuleInput {
            cinema_id: r.cinema_id,
            features: r.features,
            title_substring: r.title_substring,
            frequency: r.frequency,
            channels: vec!["email".into()],
        })
        .collect();
    crate::notification::db::replace_rules(&state.pool, auth.user_id, &input)
        .await
        .map_err(|e| {
            tracing::error!("replace_rules failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    for layer in ["email", "telegram"] {
        let _ = crate::notification::db::delete_open_batch(&state.pool, auth.user_id, layer).await;
    }
    get_rules(State(state), auth).await
}

fn validate_rules(
    rules: &[RuleRequest],
    cinemas: &std::collections::HashMap<i64, String>,
) -> Result<(), StatusCode> {
    if rules.len() > MAX_RULES {
        return Err(StatusCode::BAD_REQUEST);
    }
    let is_freq = |f: &str| {
        f == "never"
            || f == "immediately"
            || matches!(f.parse::<i32>(), Ok(d) if (1..=7).contains(&d))
    };
    for r in rules {
        if !is_freq(&r.frequency) {
            return Err(StatusCode::BAD_REQUEST);
        }
        if let Some(t) = &r.title_substring {
            if t.chars().count() > MAX_TITLE_LEN {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        for f in &r.features {
            if !FEATURE_VOCABULARY.contains(&f.as_str()) {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        if let Some(cid) = r.cinema_id {
            if !cinemas.contains_key(&cid) {
                return Err(StatusCode::BAD_REQUEST);
            }
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
            telegram_webhook_secret: None,
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
        assert_eq!(json["emailEnabled"], false);
        assert_eq!(json["telegramEnabled"], false);
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
                        r#"{"emailEnabled":true,"telegramEnabled":true,"telegramHandle":"@MyHandle","digestHour":10}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["emailEnabled"], true);
        assert_eq!(json["telegramEnabled"], true);
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
        assert_eq!(json["emailEnabled"], true);
        assert_eq!(json["telegramEnabled"], true);
        assert_eq!(json["telegramHandle"], "myhandle");
        assert_eq!(json["digestHour"], 10);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn put_preferences_accepts_valid_enablement(pool: PgPool) {
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
                    .body(axum::body::Body::from(r#"{"telegramEnabled":false}"#))
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
        crate::notification::db::get_or_create_open_batch(&pool, uid, "email", "immediately")
            .await
            .unwrap();
        crate::notification::db::get_or_create_open_batch(&pool, uid, "telegram", "immediately")
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
                        r#"{"emailEnabled":true,"telegramEnabled":true}"#,
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
                telegram_enabled: Some(true),
                telegram_handle: Some("myhandle".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        crate::notification::db::get_or_create_open_batch(&pool, uid, "telegram", "immediately")
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
        assert_eq!(json["telegramEnabled"], false);
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
        assert_eq!(json["telegramEnabled"], false);
        assert_eq!(json["telegramHandle"], serde_json::Value::Null);
    }

    async fn seed_rules_user(pool: &PgPool) -> i64 {
        let uid = crate::db::find_or_create_user(pool, "email", "rules@api.com", "rules@api.com")
            .await
            .unwrap();
        crate::notification::db::upsert_preferences(
            pool,
            uid,
            crate::notification::db::PreferenceUpdate {
                email_enabled: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        uid
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_rules_returns_empty_plus_cinemas(pool: PgPool) {
        let uid = seed_rules_user(&pool).await;
        let token = make_session(&pool, uid).await;
        let app = crate::web::router(test_state(pool));
        let resp = app
            .oneshot(
                Request::get("/api/preferences/rules")
                    .header("Cookie", format!("ov_session={token}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["rules"], serde_json::json!([]));
        let names: Vec<String> = json["cinemas"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"Cineplexx Linz".to_string()));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn put_rules_replaces_and_rolls_over(pool: PgPool) {
        let uid = seed_rules_user(&pool).await;
        crate::notification::db::get_or_create_open_batch(&pool, uid, "email", "immediately")
            .await
            .unwrap();
        let token = make_session(&pool, uid).await;
        let app = crate::web::router(test_state(pool.clone()));
        let body = r#"{"rules":[{"cinemaId":1,"features":["IMAX","Atmos"],"titleSubstring":null,"frequency":"immediately"},{"cinemaId":null,"features":[],"titleSubstring":null,"frequency":"3"}]}"#;
        let resp = app
            .oneshot(
                Request::put("/api/preferences/rules")
                    .header("Cookie", format!("ov_session={token}"))
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let json: serde_json::Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(json["rules"].as_array().unwrap().len(), 2);
        assert_eq!(json["rules"][0]["frequency"], "immediately");
        assert_eq!(json["rules"][1]["frequency"], "3");
        let n: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM notification_batch WHERE user_id=$1 AND status='pending'",
        )
        .bind(uid)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(n.0, 0, "open batches rolled over on save");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn put_rules_rejects_bad_frequency(pool: PgPool) {
        let uid = seed_rules_user(&pool).await;
        let token = make_session(&pool, uid).await;
        let app = crate::web::router(test_state(pool));
        let resp = app.oneshot(
            Request::put("/api/preferences/rules").header("Cookie", format!("ov_session={token}"))
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(r#"{"rules":[{"cinemaId":null,"features":[],"titleSubstring":null,"frequency":"sometimes"}]}"#)).unwrap()
        ).await.unwrap();
        assert_eq!(resp.status(), 400);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_rules_unauthenticated_401(pool: PgPool) {
        let app = crate::web::router(test_state(pool));
        let resp = app
            .oneshot(
                Request::get("/api/preferences/rules")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 401);
    }
}
