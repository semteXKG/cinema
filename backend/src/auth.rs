use crate::db;
use crate::web::{AppState, AppleConfig, OAuthConfig};
use axum::body::Body;
use axum::extract::{FromRef, FromRequestParts, Query, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use base64::Engine;
use chrono::Utc;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata};
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet, EndpointNotSet,
    EndpointSet, IssuerUrl, Nonce, RedirectUrl, Scope, TokenResponse,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;
use tokio::sync::Mutex;

pub struct AuthUser {
    pub user_id: i64,
    pub email: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MeResponse {
    id: i64,
    email: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProvidersResponse {
    email: bool,
    google: bool,
    apple: bool,
    github: bool,
}

#[derive(Debug, Deserialize)]
struct EmailRequest {
    email: String,
}

#[derive(Debug, Serialize)]
struct OkResponse {
    ok: bool,
}

const SESSION_COOKIE_NAME: &str = "ov_session";
const SESSION_DAYS: i64 = 30;
const STATE_COOKIE_NAME: &str = "ov_oauth_state";
const PENDING_COOKIE_NAME: &str = "ov_pending";

async fn build_session_cookie(token: &str) -> String {
    format!(
        "{name}={token}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={age}",
        name = SESSION_COOKIE_NAME,
        age = SESSION_DAYS * 86400,
    )
}

fn build_pending_cookie(token: &str) -> String {
    format!(
        "{name}={token}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=900",
        name = PENDING_COOKIE_NAME,
    )
}

fn cleared_session_cookie() -> String {
    format!(
        "{name}=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0",
        name = SESSION_COOKIE_NAME,
    )
}

fn new_token() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn redirect_to(location: &str) -> Response {
    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, location)
        .body(Body::empty())
        .unwrap()
}

fn redirect_to_with_cookie(location: &str, cookie: &str) -> Response {
    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, location)
        .header(header::SET_COOKIE, cookie)
        .body(Body::empty())
        .unwrap()
}

fn build_mailer(
    smtp: &crate::web::SmtpConfig,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, StatusCode> {
    let creds = Credentials::new(
        smtp.username.clone().unwrap_or_default(),
        smtp.password.clone(),
    );
    Ok(
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp.host)
            .map_err(|e| {
                tracing::error!("smtp relay: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .port(smtp.port)
            .credentials(creds)
            .build(),
    )
}

fn build_login_email(from: &str, to: &str, link: &str) -> Result<Message, StatusCode> {
    Message::builder()
        .from(from.parse().map_err(|e| {
            tracing::error!("invalid from: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?)
        .to(to.parse().map_err(|e| {
            tracing::error!("invalid to: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?)
        .subject("OV-Kino Linz — Sign in")
        .body(format!(
            "Hi!\n\n\
             A sign-in link was requested for OV-Kino Linz.\n\n\
             Sign in:\n{link}\n\n\
             This link is valid for 15 minutes and can be used once. \
             After signing in, you'll stay logged in for 30 days.\n\n\
             If you didn't request this, you can ignore this email."
        ))
        .map_err(|e| {
            tracing::error!("build email: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

// ---------- handlers ----------

async fn post_email(
    State(state): State<AppState>,
    Json(body): Json<EmailRequest>,
) -> Result<Response, StatusCode> {
    let smtp = state
        .smtp_config
        .as_ref()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;
    let token = new_token();
    let pending_cookie = build_pending_cookie(&token);
    let expires = Utc::now() + chrono::Duration::minutes(15);
    db::insert_email_token(&state.pool, &body.email, &token, expires)
        .await
        .map_err(|e| {
            tracing::error!("insert_email_token failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let link = format!("{}/api/auth/verify?token={}", state.base_url, token);
    let email = build_login_email(&smtp.from, &body.email, &link)?;
    let mailer = build_mailer(smtp)?;
    if let Err(e) = mailer.send(email).await {
        tracing::error!("send email failed: {e}");
    }
    // Always return ok to avoid email enumeration
    Ok((
        StatusCode::OK,
        [(header::SET_COOKIE, pending_cookie)],
        Json(OkResponse { ok: true }),
    )
        .into_response())
}

async fn get_verify(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, StatusCode> {
    let token = params.get("token").ok_or(StatusCode::BAD_REQUEST)?;
    let email = db::consume_email_token(&state.pool, token)
        .await
        .map_err(|e| {
            tracing::error!("consume_email_token failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    match email {
        Some(_) => Ok(redirect_to(&format!("{}/?login=confirmed", state.base_url))),
        None => Ok(redirect_to(&format!(
            "{}/?error=invalid_token",
            state.base_url
        ))),
    }
}

async fn get_me(auth: AuthUser) -> Result<Json<MeResponse>, StatusCode> {
    Ok(Json(MeResponse {
        id: auth.user_id,
        email: auth.email,
    }))
}

async fn post_logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = read_cookie(&headers, SESSION_COOKIE_NAME) {
        if let Err(e) = db::delete_session(&state.pool, &token).await {
            tracing::error!("delete_session failed: {e}");
        }
    }
    (
        StatusCode::OK,
        [(header::SET_COOKIE, cleared_session_cookie())],
        Json(OkResponse { ok: true }),
    )
        .into_response()
}

async fn get_providers(State(state): State<AppState>) -> Json<ProvidersResponse> {
    Json(ProvidersResponse {
        email: state.smtp_config.is_some(),
        google: state.google_oauth.is_some(),
        apple: state.apple_oauth.is_some(),
        github: state.github_oauth.is_some(),
    })
}

// ---------- SSO ----------

type OidcClient = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

static OIDC_CLIENTS: OnceLock<Mutex<HashMap<String, Result<OidcClient, String>>>> = OnceLock::new();

fn oidc_issuer(provider: &str) -> Result<String, StatusCode> {
    match provider {
        "google" => Ok("https://accounts.google.com".into()),
        "apple" => Ok("https://appleid.apple.com".into()),
        _ => Err(StatusCode::NOT_FOUND),
    }
}

fn apple_client_secret(cfg: &AppleConfig) -> Result<String, StatusCode> {
    // Apple's client_secret is a short-lived JWT signed with the registered private key
    let now = Utc::now();
    let exp = now + chrono::Duration::days(180);
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
    header.kid = Some(cfg.key_id.clone());
    let claims = serde_json::json!({
        "iss": cfg.team_id,
        "iat": now.timestamp(),
        "exp": exp.timestamp(),
        "aud": "https://appleid.apple.com",
        "sub": cfg.client_id,
    });
    let key = jsonwebtoken::EncodingKey::from_ec_pem(cfg.private_key.as_bytes()).map_err(|e| {
        tracing::error!("invalid Apple private key: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    jsonwebtoken::encode(&header, &claims, &key).map_err(|e| {
        tracing::error!("failed to sign Apple client secret: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

async fn oidc_client(state: &AppState, provider: &str) -> Result<OidcClient, StatusCode> {
    let (client_id, client_secret) = match provider {
        "google" => {
            let c = state
                .google_oauth
                .as_ref()
                .ok_or(StatusCode::NOT_IMPLEMENTED)?;
            (c.client_id.clone(), Some(c.client_secret.clone()))
        }
        "apple" => {
            let c = state
                .apple_oauth
                .as_ref()
                .ok_or(StatusCode::NOT_IMPLEMENTED)?;
            (c.client_id.clone(), Some(apple_client_secret(c)?))
        }
        _ => return Err(StatusCode::NOT_FOUND),
    };
    let cache_key = format!("{provider}|{client_id}|{}", state.base_url);
    let cache = OIDC_CLIENTS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache.lock().await.get(&cache_key) {
        return cached.clone().map_err(|e| {
            tracing::error!("oidc client cache hit with prior failure: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        });
    }
    let issuer = oidc_issuer(provider)?;
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| {
            tracing::error!("failed to build http client: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let metadata = CoreProviderMetadata::discover_async(IssuerUrl::new(issuer).unwrap(), &http)
        .await
        .map_err(|e| {
            tracing::error!("oidc discovery failed for {provider}: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let client = CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(client_id.clone()),
        client_secret.map(ClientSecret::new),
    )
    .set_redirect_uri(
        RedirectUrl::new(format!(
            "{}/api/auth/sso/{}/callback",
            state.base_url, provider
        ))
        .map_err(|e| {
            tracing::error!("invalid redirect url: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?,
    );
    let mut guard = cache.lock().await;
    guard.insert(cache_key, Ok(client.clone()));
    Ok(client)
}

async fn sso_initiate_oidc(state: &AppState, provider: &str) -> Result<Response, StatusCode> {
    let client = oidc_client(state, provider).await?;
    // Note: the `openid` scope is added automatically by the client, so we only
    // request `email` and `profile` explicitly.
    let (auth_url, csrf_token, nonce) = client
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .add_scope(Scope::new("email".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .url();
    // Store both CSRF state and nonce in the cookie so the callback can verify both
    let state_value = format!("{}:{}", csrf_token.secret(), nonce.secret());
    let state_cookie = format!(
        "{name}={value}; Secure; SameSite=Lax; Path=/; Max-Age=600",
        name = STATE_COOKIE_NAME,
        value = state_value,
    );
    Ok(Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, auth_url.to_string())
        .header(header::SET_COOKIE, state_cookie)
        .body(Body::empty())
        .unwrap())
}

async fn sso_google(State(state): State<AppState>) -> Result<Response, StatusCode> {
    sso_initiate_oidc(&state, "google").await
}

async fn sso_apple(State(state): State<AppState>) -> Result<Response, StatusCode> {
    sso_initiate_oidc(&state, "apple").await
}

async fn sso_github(State(state): State<AppState>) -> Result<Response, StatusCode> {
    let oauth = state
        .github_oauth
        .as_ref()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;
    let (auth_url, csrf_token) = oauth2_auth_url("github", oauth, &state.base_url);
    let state_cookie = format!(
        "{name}={secret}; Secure; SameSite=Lax; Path=/; Max-Age=600",
        name = STATE_COOKIE_NAME,
        secret = csrf_token.secret(),
    );
    Ok(Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, auth_url)
        .header(header::SET_COOKIE, state_cookie)
        .body(Body::empty())
        .unwrap())
}

// GitHub doesn't do OIDC, so it gets a tiny hand-rolled auth URL builder
fn oauth2_auth_url(provider: &str, cfg: &OAuthConfig, base_url: &str) -> (String, CsrfToken) {
    let csrf = CsrfToken::new_random();
    let params = [
        ("client_id", cfg.client_id.clone()),
        (
            "redirect_uri",
            format!("{base_url}/api/auth/sso/{provider}/callback"),
        ),
        ("scope", "user:email".to_string()),
        ("state", csrf.secret().clone()),
    ];
    let query = params
        .iter()
        .map(|(k, v)| format!("{k}={}", urlencode(v)))
        .collect::<Vec<_>>()
        .join("&");
    (
        format!("https://github.com/login/oauth/authorize?{query}"),
        csrf,
    )
}

fn urlencode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => {
                let mut b = [0u8; 4];
                c.encode_utf8(&mut b)
                    .bytes()
                    .map(|b| format!("%{b:02X}"))
                    .collect()
            }
        })
        .collect()
}

fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookie| {
            cookie
                .split(';')
                .map(|s| s.trim())
                .find_map(|c| c.strip_prefix(&format!("{name}=")))
                .map(|t| t.to_string())
        })
}

// OIDC callback shared by Google and Apple. The id_token signature is verified
// against the provider's JWKS and the nonce is checked.
async fn sso_callback_oidc(
    state: &AppState,
    headers: &HeaderMap,
    params: &HashMap<String, String>,
    provider: &str,
) -> Result<Response, StatusCode> {
    // Validate CSRF state + nonce from cookie (stored as "csrf:nonce")
    let cookie_state = read_cookie(headers, STATE_COOKIE_NAME);
    let (expected_csrf, expected_nonce) = cookie_state
        .as_deref()
        .and_then(|v| v.split_once(':'))
        .map(|(c, n)| (c.to_string(), n.to_string()))
        .unwrap_or_default();
    let state_param = params.get("state").cloned().unwrap_or_default();
    if state_param.is_empty() || expected_csrf.is_empty() || state_param != expected_csrf {
        return Ok(redirect_to(&format!(
            "{}/?error=invalid_state",
            state.base_url
        )));
    }
    let client = oidc_client(state, provider).await?;
    let code = params.get("code").cloned().unwrap_or_default();
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| {
            tracing::error!("failed to build http client: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let token_req = client
        .exchange_code(AuthorizationCode::new(code))
        .map_err(|e| {
            tracing::error!("oauth config error for {provider}: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let token_res = token_req.request_async(&http).await.map_err(|e| {
        tracing::error!("oauth token exchange failed for {provider}: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    // Extract and verify the id_token signature against the provider JWKS
    let id_token = token_res.id_token().ok_or_else(|| {
        tracing::error!("no id_token returned by {provider}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let nonce = Nonce::new(expected_nonce);
    let verifier = client.id_token_verifier();
    let claims = id_token.claims(&verifier, &nonce).map_err(|e| {
        tracing::error!("id_token verification failed for {provider}: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let sub = claims.subject().to_string();
    // Apple only returns email on the first login; store whatever we have.
    // Google: only use the email claim when the provider verified it.
    let email = if provider == "google" && claims.email_verified() != Some(true) {
        format!("{provider}-{sub}@unknown")
    } else {
        claims
            .email()
            .map(|e| e.as_str().to_string())
            .unwrap_or_else(|| format!("{provider}-{sub}@unknown"))
    };
    let user_id = db::find_or_create_user(&state.pool, provider, &sub, &email)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let session_token = new_token();
    let expires = Utc::now() + chrono::Duration::days(SESSION_DAYS);
    db::create_session(&state.pool, user_id, &session_token, expires)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(redirect_to_with_cookie(
        &state.base_url,
        &build_session_cookie(&session_token).await,
    ))
}

async fn sso_google_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, StatusCode> {
    sso_callback_oidc(&state, &headers, &params, "google").await
}

async fn sso_apple_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, StatusCode> {
    sso_callback_oidc(&state, &headers, &params, "apple").await
}

async fn sso_github_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, StatusCode> {
    let oauth = state
        .github_oauth
        .as_ref()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;
    let expected_state = read_cookie(&headers, STATE_COOKIE_NAME);
    let state_param = params.get("state").cloned().unwrap_or_default();
    if state_param.is_empty() || expected_state.as_deref() != Some(state_param.as_str()) {
        return Ok(redirect_to(&format!(
            "{}/?error=invalid_state",
            state.base_url
        )));
    }
    let code = params.get("code").cloned().unwrap_or_default();
    let http = reqwest::Client::new();
    // Exchange code for access token
    let token_resp: serde_json::Value = http
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("client_id", oauth.client_id.clone()),
            ("client_secret", oauth.client_secret.clone()),
            ("code", code.clone()),
        ])
        .send()
        .await
        .map_err(|e| {
            tracing::error!("github token exchange failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .json()
        .await
        .map_err(|e| {
            tracing::error!("github token exchange parse failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let access_token = token_resp["access_token"]
        .as_str()
        .unwrap_or("")
        .to_string();
    if access_token.is_empty() {
        return Ok(redirect_to(&format!(
            "{}/?error=oauth_failed",
            state.base_url
        )));
    }
    // Fetch the user's numeric id
    let user: serde_json::Value = http
        .get("https://api.github.com/user")
        .header("User-Agent", "ov-kino-linz")
        .bearer_auth(&access_token)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("github user fetch failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .error_for_status()
        .map_err(|e| {
            tracing::error!("github user fetch error status: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .json()
        .await
        .map_err(|e| {
            tracing::error!("github user parse failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let id = match user["id"].as_i64() {
        Some(id) => id.to_string(),
        None => {
            tracing::error!("github user response missing id");
            return Ok(redirect_to(&format!(
                "{}/?error=oauth_failed",
                state.base_url
            )));
        }
    };
    // Fetch verified primary email
    let emails: Vec<serde_json::Value> = http
        .get("https://api.github.com/user/emails")
        .header("User-Agent", "ov-kino-linz")
        .bearer_auth(&access_token)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("github emails fetch failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .error_for_status()
        .map_err(|e| {
            tracing::error!("github emails fetch error status: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .json()
        .await
        .map_err(|e| {
            tracing::error!("github emails parse failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let email = emails
        .iter()
        .find(|e| e["primary"].as_bool() == Some(true) && e["verified"].as_bool() == Some(true))
        .and_then(|e| e["email"].as_str())
        .unwrap_or("")
        .to_string();
    let user_id = db::find_or_create_user(&state.pool, "github", &id, &email)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let session_token = new_token();
    let expires = Utc::now() + chrono::Duration::days(SESSION_DAYS);
    db::create_session(&state.pool, user_id, &session_token, expires)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(redirect_to_with_cookie(
        &state.base_url,
        &build_session_cookie(&session_token).await,
    ))
}

// ---------- extractor ----------

impl<S: Sync> FromRequestParts<S> for AuthUser
where
    AppState: FromRef<S>,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        let cookie_header = parts
            .headers
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = cookie_header
            .split(';')
            .map(|s| s.trim())
            .find_map(|c| c.strip_prefix(&format!("{SESSION_COOKIE_NAME}=")))
            .map(|t| t.to_string());
        match token {
            Some(t) => {
                let row = db::lookup_session(&app_state.pool, &t).await.map_err(|e| {
                    tracing::error!("lookup_session failed: {e}");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
                match row {
                    Some((user_id, email)) => Ok(AuthUser { user_id, email }),
                    None => Err(StatusCode::UNAUTHORIZED),
                }
            }
            None => Err(StatusCode::UNAUTHORIZED),
        }
    }
}

// ---------- router ----------

pub fn auth_router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/email", post(post_email))
        .route("/api/auth/verify", get(get_verify))
        .route("/api/auth/sso/google", get(sso_google))
        .route("/api/auth/sso/google/callback", get(sso_google_callback))
        .route("/api/auth/sso/apple", get(sso_apple))
        .route("/api/auth/sso/apple/callback", get(sso_apple_callback))
        .route("/api/auth/sso/github", get(sso_github))
        .route("/api/auth/sso/github/callback", get(sso_github_callback))
        .route("/api/auth/me", get(get_me))
        .route("/api/auth/logout", post(post_logout))
        .route("/api/auth/providers", get(get_providers))
}

#[cfg(test)]
mod tests {
    use super::*;
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
            smtp_config: None,
            google_oauth: None,
            apple_oauth: None,
            github_oauth: None,
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn providers_endpoint(pool: PgPool) {
        let state = test_state(pool);
        let app = Router::new().merge(auth_router()).with_state(state);
        let resp = app
            .oneshot(
                Request::get("/api/auth/providers")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["email"], false);
        assert_eq!(json["google"], false);
        assert_eq!(json["apple"], false);
        assert_eq!(json["github"], false);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn me_unauthenticated(pool: PgPool) {
        let state = test_state(pool);
        let app = Router::new().merge(auth_router()).with_state(state);
        let resp = app
            .oneshot(
                Request::get("/api/auth/me")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 401);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn me_authenticated(pool: PgPool) {
        let uid = db::find_or_create_user(&pool, "email", "a@b.com", "a@b.com")
            .await
            .unwrap();
        let session_token = new_token();
        let expires = Utc::now() + chrono::Duration::days(30);
        db::create_session(&pool, uid, &session_token, expires)
            .await
            .unwrap();
        let state = test_state(pool);
        let app = Router::new().merge(auth_router()).with_state(state);
        let resp = app
            .oneshot(
                Request::get("/api/auth/me")
                    .header("Cookie", format!("ov_session={session_token}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["id"], 1);
        assert_eq!(json["email"], "a@b.com");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn email_endpoint_returns_ok_always(pool: PgPool) {
        let state = test_state(pool);
        // None smtp -> 501
        {
            let app = Router::new().merge(auth_router()).with_state(state.clone());
            let resp = app
                .oneshot(
                    Request::post("/api/auth/email")
                        .header("Content-Type", "application/json")
                        .body(axum::body::Body::from(r#"{"email":"x@y.com"}"#))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), 501);
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn verify_consumes_token_but_issues_no_session(pool: PgPool) {
        use rand::Rng;
        let state = test_state(pool.clone());
        let app = Router::new().merge(auth_router()).with_state(state);
        let mut rng = rand::thread_rng();
        let token_bytes: [u8; 32] = rng.gen();
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);
        let expires = Utc::now() + chrono::Duration::minutes(15);
        db::insert_email_token(&pool, "a@b.com", &token, expires)
            .await
            .unwrap();

        let resp = app
            .oneshot(
                Request::get(format!("/api/auth/verify?token={token}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 302);
        let location = resp.headers().get("location").unwrap().to_str().unwrap();
        assert!(location.contains("login=confirmed"));
        // no session cookie is set
        assert!(!resp.headers().contains_key("set-cookie"));
        // token is consumed
        let st = db::lookup_email_token(&pool, &token)
            .await
            .unwrap()
            .unwrap();
        assert!(st.used);
    }

    #[test]
    fn pending_cookie_format() {
        let cookie = build_pending_cookie("tok123");
        assert_eq!(
            cookie,
            "ov_pending=tok123; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=900"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn verify_invalid_token_redirects_with_error(pool: PgPool) {
        let state = test_state(pool);
        let app = Router::new().merge(auth_router()).with_state(state);
        let resp = app
            .oneshot(
                Request::get("/api/auth/verify?token=bad")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 302);
        let location = resp.headers().get("location").unwrap().to_str().unwrap();
        assert!(location.contains("error=invalid_token"));
    }

    #[tokio::test]
    async fn mailer_speaks_starttls_not_implicit_tls() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        // Local plaintext SMTP server that greets, then captures the first
        // bytes the client sends. A STARTTLS client sends "EHLO ..." in the
        // clear; an implicit-TLS client sends a TLS ClientHello (record type
        // 0x16) which Resend's port 587 rejects.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            sock.write_all(b"220 test ESMTP\r\n").await.unwrap();
            let mut buf = [0u8; 64];
            let n = sock.read(&mut buf).await.unwrap();
            buf[..n].to_vec()
        });

        let smtp = crate::web::SmtpConfig {
            host: addr.ip().to_string(),
            port: addr.port(),
            username: Some("resend".into()),
            password: "test".into(),
            from: "noreply@example.com".into(),
        };
        let mailer = build_mailer(&smtp).unwrap();
        let email = Message::builder()
            .from("noreply@example.com".parse().unwrap())
            .to("x@y.com".parse().unwrap())
            .subject("test")
            .body("hi".to_string())
            .unwrap();
        // Connection will die after the client's first command; that's fine.
        let _ = mailer.send(email).await;

        let first_bytes = server.await.unwrap();
        assert!(
            first_bytes.starts_with(b"EHLO") || first_bytes.starts_with(b"HELO"),
            "client sent {:?}, expected a plaintext EHLO (STARTTLS), not an implicit TLS handshake",
            String::from_utf8_lossy(&first_bytes)
        );
    }

    #[test]
    fn login_email_has_context_and_expiry() {
        // `formatted()` emits quoted-printable: "=" is "=3D", soft line wraps
        // are "=\n". Decode it so assertions work on the plain body.
        fn decode_qp(input: &str) -> String {
            let mut out = String::new();
            let mut chars = input.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '=' {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    } else {
                        let hex: String = chars.by_ref().take(2).collect();
                        out.push(
                            u8::from_str_radix(&hex, 16)
                                .ok()
                                .map(char::from)
                                .unwrap_or('?'),
                        );
                    }
                } else {
                    out.push(c);
                }
            }
            out
        }

        let msg = build_login_email(
            "noreply@cinema.k-labs.app",
            "user@example.com",
            "https://cinema.k-labs.app/api/auth/verify?token=abc123",
        )
        .unwrap();
        let body = decode_qp(&String::from_utf8_lossy(&msg.formatted()));
        assert!(
            body.contains("https://cinema.k-labs.app/api/auth/verify?token=abc123"),
            "body should contain the sign-in link: {body}"
        );
        assert!(
            body.contains("15 minutes"),
            "body should mention the link expiry: {body}"
        );
        assert!(
            body.contains("30 days"),
            "body should say the session lasts 30 days: {body}"
        );
        assert!(
            body.contains("OV-Kino Linz"),
            "body should identify the service: {body}"
        );
    }
}
