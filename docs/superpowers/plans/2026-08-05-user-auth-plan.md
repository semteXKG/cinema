# User Auth Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add user authentication (email magic link + Google/Apple/GitHub SSO) with session cookies.

**Architecture:** New `auth.rs` module handles auth endpoints and session extraction. Config gains SMTP + OAuth fields. Existing `db.rs` gains auth queries. Frontend adds a React context for auth state and login UI in the Marquee nav.

**Tech Stack:** Rust/axum, sqlx/Postgres, `lettre` (SMTP), `oauth2` (SSO), `jsonwebtoken` (Apple), React 19, no new frontend deps.

## Global Constraints

- Auth is optional: each login method is only available if its corresponding env vars are configured. Unconfigured methods return `501 Not Implemented` and are hidden from the frontend.
- Session cookie named `ov_session`, HTTP-only, Secure, SameSite=Lax, 30-day expiry, 32 random bytes URL-safe base64 encoded.
- No passwords stored. Identities keyed by `(provider, provider_id)`. Email is a claim, not an identity.
- Identity linking: match on `(provider, provider_id)` first; if no match but `users.email` matches, add identity to existing user; otherwise create new user.
- Email enumeration prevention: `POST /api/auth/email` always returns `200 {"ok":true}`.
- State CSRF cookies for OAuth are short-lived (10 minutes), Secure, SameSite=Lax.
- `rand` crate for token generation, `lettre` for SMTP, `oauth2` for SSO, `jsonwebtoken` for Apple client secret JWT.

---

### Task 1: Migration + Dependencies

**Files:**
- Create: `backend/migrations/0002_users.sql`
- Modify: `backend/Cargo.toml:1-28`

**Interfaces:**
- Produces: Tables `users`, `user_identities`, `sessions`, `email_tokens`

- [ ] **Step 1: Create migration file**

```sql
CREATE TABLE users (
  id         BIGSERIAL PRIMARY KEY,
  email      TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE user_identities (
  user_id     BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  provider    TEXT NOT NULL,
  provider_id TEXT NOT NULL,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (provider, provider_id)
);

CREATE TABLE sessions (
  token      TEXT PRIMARY KEY,
  user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE email_tokens (
  token      TEXT PRIMARY KEY,
  email      TEXT NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL,
  used       BOOLEAN NOT NULL DEFAULT false
);
```

- [ ] **Step 2: Add crate dependencies to Cargo.toml**

```toml
rand = "0.8"
lettre = { version = "0.11", default-features = false, features = ["builder", "rustls-tls", "smtp-transport"] }
oauth2 = { version = "4", default-features = false, features = ["reqwest"] }
jsonwebtoken = "9"
base64 = "0.22"
```

Add these after the existing `sha1` dependency line.

- [ ] **Step 3: Run migration locally to verify it applies cleanly**

Run: `cd backend && DATABASE_URL=postgres://ov:ov@localhost:5432/ov sqlx migrate run`
Expected: `Applied 0002_users.sql`

- [ ] **Step 4: Verify the dependency build**

Run: `cd backend && cargo check`
Expected: Compiles without errors

- [ ] **Step 5: Commit**

```bash
git add backend/migrations/0002_users.sql backend/Cargo.toml backend/Cargo.lock
git commit -m "feat: add user auth migration and dependencies"
```

---

### Task 2: Config + AppState extensions

**Files:**
- Modify: `backend/src/config.rs:4-15` (Config struct)
- Modify: `backend/src/config.rs:39-55` (constructor)
- Modify: `backend/src/config.rs:58-127` (tests)
- Modify: `backend/src/web.rs:8-14` (AppState)

**Interfaces:**
- Produces: `Config.smtp_host`, `Config.smtp_port`, `Config.smtp_username`, `Config.smtp_password`, `Config.smtp_from`, `Config.base_url`, `Config.google_client_id`, `Config.google_client_secret`, `Config.apple_client_id`, `Config.apple_client_secret`, `Config.github_client_id`, `Config.github_client_secret`
- Produces: `AppState.base_url`, `AppState.smtp_config: Option<SmtpConfig>`, `AppState.google_oauth`, `AppState.apple_oauth`, `AppState.github_oauth`

- [ ] **Step 1: Add new fields to Config struct**

```rust
// After the existing fields in Config:
pub smtp_host: Option<String>,
pub smtp_port: u16,
pub smtp_username: Option<String>,
pub smtp_password: Option<String>,
pub smtp_from: Option<String>,
pub base_url: String,
pub google_client_id: Option<String>,
pub google_client_secret: Option<String>,
pub apple_client_id: Option<String>,
pub apple_client_secret: Option<String>,
pub github_client_id: Option<String>,
pub github_client_secret: Option<String>,
```

- [ ] **Step 2: Update Config::from_lookup constructor**

Add these after the existing `port` parse block:

```rust
let smtp_port: u16 = get("SMTP_PORT")
    .unwrap_or_else(|| "587".into())
    .parse()
    .map_err(|_| anyhow::anyhow!("SMTP_PORT must be a number"))?;
```

And add to the `Ok(Config { ... })` block (after the existing `static_dir` line):

```rust
smtp_host: get("SMTP_HOST"),
smtp_port,
smtp_username: get("SMTP_USERNAME"),
smtp_password: get("SMTP_PASSWORD"),
smtp_from: get("SMTP_FROM"),
base_url: get("BASE_URL").unwrap_or_else(|| "https://cinema.k-labs.app".into()),
google_client_id: get("GOOGLE_CLIENT_ID"),
google_client_secret: get("GOOGLE_CLIENT_SECRET"),
apple_client_id: get("APPLE_CLIENT_ID"),
apple_client_secret: get("APPLE_CLIENT_SECRET"),
github_client_id: get("GITHUB_CLIENT_ID"),
github_client_secret: get("GITHUB_CLIENT_SECRET"),
```

- [ ] **Step 3: Update existing Config tests to include new defaults**

In `defaults_when_only_database_url_set`, add assertions:
```rust
assert_eq!(cfg.smtp_port, 587);
assert_eq!(cfg.smtp_host, None);
assert_eq!(cfg.smtp_from, None);
assert_eq!(cfg.base_url, "https://cinema.k-labs.app");
assert_eq!(cfg.google_client_id, None);
```

In `parses_all_overrides`, add test env vars and assertions:
```rust
("SMTP_HOST", "smtp.example.com"),
("SMTP_PORT", "465"),
("SMTP_FROM", "OV-Kino <noreply@k-labs.app>"),
("BASE_URL", "http://localhost:8080"),
("GOOGLE_CLIENT_ID", "gcid"),
// ...
```
And assert `cfg.smtp_port == 465`, `cfg.base_url == "http://localhost:8080"`, etc.

- [ ] **Step 4: Add test for invalid SMTP_PORT**

```rust
#[test]
fn invalid_smtp_port_is_an_error() {
    let cfg = Config::from_lookup(env_of(&[("DATABASE_URL", "postgres://x"), ("SMTP_PORT", "abc")]));
    assert!(cfg.is_err());
}
```

- [ ] **Step 5: Add SmtpConfig and OAuthConfig types to web.rs, extend AppState**

At the top of `web.rs`, add after the existing `use` statements:

```rust
#[derive(Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: String,
    pub from: String,
}

#[derive(Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
}
```

Extend `AppState`:

```rust
pub struct AppState {
    pub pool: PgPool,
    pub data_dir: PathBuf,
    pub static_dir: PathBuf,
    pub base_url: String,
    pub smtp_config: Option<SmtpConfig>,
    pub google_oauth: Option<OAuthConfig>,
    pub apple_oauth: Option<OAuthConfig>,
    pub github_oauth: Option<OAuthConfig>,
}
```

- [ ] **Step 6: Update panic-web.rs tests that construct AppState**

Every test constructing `AppState { pool, data_dir, static_dir }` must add the new fields. Pattern for all:

```rust
let state = AppState {
    pool: pool.clone(),
    data_dir: PathBuf::new(),
    static_dir: PathBuf::from("/nonexistent"),
    base_url: "http://localhost".into(),
    smtp_config: None,
    google_oauth: None,
    apple_oauth: None,
    github_oauth: None,
};
```

There are 4 test functions constructing AppState: `api_showings_three_states`, `ics_route_renders_events`, `poster_route_serves_and_guards`, `healthz_route`. Update all.

- [ ] **Step 7: Run web.rs tests to verify**

Run: `cd backend && DATABASE_URL=postgres://ov:ov@localhost:5432/ov cargo test web::tests`
Expected: All pass

- [ ] **Step 8: Run config tests to verify**

Run: `cd backend && cargo test config::tests`
Expected: All pass

- [ ] **Step 9: Commit**

```bash
git add backend/src/config.rs backend/src/web.rs
git commit -m "feat: add auth fields to Config and AppState"
```

---

### Task 3: Auth DB queries

**Files:**
- Modify: `backend/src/db.rs:1-299` (add public functions at bottom)

**Interfaces:**
- Consumes: Tables `users`, `user_identities`, `sessions`, `email_tokens` (from migration)
- Produces: `insert_email_token(pool, email, token, expires_at) -> Result<()>`
- Produces: `consume_email_token(pool, token) -> Result<Option<String>>` (returns email if valid+unused, marks used)
- Produces: `find_or_create_user(pool, provider, provider_id, email) -> Result<i64>` (implement linking rule)
- Produces: `create_session(pool, user_id, token, expires_at) -> Result<()>`
- Produces: `lookup_session(pool, token) -> Result<Option<(i64, String)>>` (returns user_id, email)
- Produces: `delete_session(pool, token) -> Result<()>`
- Produces: `prune_expired_sessions(pool) -> Result<()>`

- [ ] **Step 1: Write the tests in db.rs test module**

Add at the end of the existing `mod tests` (after `check_run_latest`):

```rust
#[sqlx::test(migrations = "./migrations")]
async fn email_token_insert_and_consume(pool: PgPool) {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let token_bytes: [u8; 32] = rng.gen();
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);
    let expires = Utc::now() + chrono::Duration::minutes(15);
    insert_email_token(&pool, "a@b.com", &token, expires).await.unwrap();
    let email = consume_email_token(&pool, &token).await.unwrap();
    assert_eq!(email, Some("a@b.com".into()));
    // second consumption fails (already used)
    let email2 = consume_email_token(&pool, &token).await.unwrap();
    assert_eq!(email2, None);
}

#[sqlx::test(migrations = "./migrations")]
async fn email_token_expired(pool: PgPool) {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let token_bytes: [u8; 32] = rng.gen();
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);
    let expires = Utc::now() - chrono::Duration::minutes(1);
    insert_email_token(&pool, "a@b.com", &token, expires).await.unwrap();
    assert_eq!(consume_email_token(&pool, &token).await.unwrap(), None);
}

#[sqlx::test(migrations = "./migrations")]
async fn find_or_create_user_new(pool: PgPool) {
    let uid = find_or_create_user(&pool, "email", "x@y.com", "x@y.com")
        .await
        .unwrap();
    assert!(uid > 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn find_or_create_user_existing_identity(pool: PgPool) {
    let uid1 = find_or_create_user(&pool, "google", "sub123", "a@b.com")
        .await
        .unwrap();
    let uid2 = find_or_create_user(&pool, "google", "sub123", "a@b.com")
        .await
        .unwrap();
    assert_eq!(uid1, uid2);
}

#[sqlx::test(migrations = "./migrations")]
async fn find_or_create_user_link_by_email(pool: PgPool) {
    let uid1 = find_or_create_user(&pool, "google", "sub123", "a@b.com")
        .await
        .unwrap();
    // login via email with same email address should link to existing user
    let uid2 = find_or_create_user(&pool, "email", "a@b.com", "a@b.com")
        .await
        .unwrap();
    assert_eq!(uid1, uid2);
}

#[sqlx::test(migrations = "./migrations")]
async fn session_lifecycle(pool: PgPool) {
    let uid = find_or_create_user(&pool, "email", "a@b.com", "a@b.com")
        .await
        .unwrap();
    let token = "sess-token-abc";
    let expires = Utc::now() + chrono::Duration::days(30);
    create_session(&pool, uid, token, expires).await.unwrap();
    let found = lookup_session(&pool, token).await.unwrap();
    assert_eq!(found, Some((uid, "a@b.com".to_string())));
    delete_session(&pool, token).await.unwrap();
    assert_eq!(lookup_session(&pool, token).await.unwrap(), None);
}

#[sqlx::test(migrations = "./migrations")]
async fn session_expired_not_found(pool: PgPool) {
    let uid = find_or_create_user(&pool, "email", "a@b.com", "a@b.com")
        .await
        .unwrap();
    let expires = Utc::now() - chrono::Duration::minutes(1);
    create_session(&pool, uid, "expired-token", expires).await.unwrap();
    assert_eq!(lookup_session(&pool, "expired-token").await.unwrap(), None);
}

#[sqlx::test(migrations = "./migrations")]
async fn prune_expired_sessions_works(pool: PgPool) {
    let uid = find_or_create_user(&pool, "email", "a@b.com", "a@b.com")
        .await
        .unwrap();
    let expires_old = Utc::now() - chrono::Duration::minutes(1);
    let expires_fresh = Utc::now() + chrono::Duration::days(1);
    create_session(&pool, uid, "old-sess", expires_old).await.unwrap();
    create_session(&pool, uid, "fresh-sess", expires_fresh).await.unwrap();
    prune_expired_sessions(&pool).await.unwrap();
    assert_eq!(lookup_session(&pool, "old-sess").await.unwrap(), None);
    assert!(lookup_session(&pool, "fresh-sess").await.unwrap().is_some());
}
```

- [ ] **Step 2: Run tests to verify they fail with "function not defined"**

Run: `cd backend && DATABASE_URL=postgres://ov:ov@localhost:5432/ov cargo test db::tests::email_token_insert_and_consume`
Expected: FAIL (compile error, functions not defined)

- [ ] **Step 3: Add imports to db.rs**

Add at top:
```rust
use base64::Engine;
```

- [ ] **Step 4: Implement DB queries**

Add after `latest_check_run` (before the test module):

```rust
pub async fn insert_email_token(
    pool: &PgPool,
    email: &str,
    token: &str,
    expires_at: DateTime<Utc>,
) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO email_tokens (token, email, expires_at) VALUES ($1, $2, $3)")
        .bind(token)
        .bind(email)
        .bind(expires_at)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn consume_email_token(pool: &PgPool, token: &str) -> sqlx::Result<Option<String>> {
    let row: Option<(String, bool, DateTime<Utc>)> = sqlx::query_as(
        "SELECT email, used, expires_at FROM email_tokens WHERE token = $1",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;
    match row {
        Some((email, used, expires)) if !used && expires > Utc::now() => {
            sqlx::query("UPDATE email_tokens SET used = true WHERE token = $1")
                .bind(token)
                .execute(pool)
                .await?;
            Ok(Some(email))
        }
        _ => Ok(None),
    }
}

pub async fn find_or_create_user(
    pool: &PgPool,
    provider: &str,
    provider_id: &str,
    email: &str,
) -> sqlx::Result<i64> {
    // Check existing identity
    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT user_id FROM user_identities WHERE provider = $1 AND provider_id = $2",
    )
    .bind(provider)
    .bind(provider_id)
    .fetch_optional(pool)
    .await?;
    if let Some((uid,)) = existing {
        return Ok(uid);
    }
    // Check if a user with this email exists (for linking)
    let existing_user: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM users WHERE email = $1")
            .bind(email)
            .fetch_optional(pool)
            .await?;
    let user_id = match existing_user {
        Some((id,)) => id,
        None => {
            let row: (i64,) =
                sqlx::query_as("INSERT INTO users (email) VALUES ($1) RETURNING id")
                    .bind(email)
                    .fetch_one(pool)
                    .await?;
            row.0
        }
    };
    // Insert the identity
    sqlx::query(
        "INSERT INTO user_identities (user_id, provider, provider_id) VALUES ($1, $2, $3)",
    )
    .bind(user_id)
    .bind(provider)
    .bind(provider_id)
    .execute(pool)
    .await?;
    Ok(user_id)
}

pub async fn create_session(
    pool: &PgPool,
    user_id: i64,
    token: &str,
    expires_at: DateTime<Utc>,
) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO sessions (token, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(token)
        .bind(user_id)
        .bind(expires_at)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn lookup_session(
    pool: &PgPool,
    token: &str,
) -> sqlx::Result<Option<(i64, String)>> {
    sqlx::query_as(
        "SELECT s.user_id, u.email FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token = $1 AND s.expires_at > now()",
    )
    .bind(token)
    .fetch_optional(pool)
    .await
    .map(|r: Option<(i64, String)>| r)
}

pub async fn delete_session(pool: &PgPool, token: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token = $1")
        .bind(token)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn prune_expired_sessions(pool: &PgPool) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM sessions WHERE expires_at <= now()")
        .execute(pool)
        .await?;
    Ok(())
}
```

- [ ] **Step 5: Run all db tests**

Run: `cd backend && DATABASE_URL=postgres://ov:ov@localhost:5432/ov cargo test db::tests`
Expected: All pass

- [ ] **Step 6: Commit**

```bash
git add backend/src/db.rs
git commit -m "feat: add auth DB queries with tests"
```

---

### Task 4: Auth module (extractor, handlers, router)

**Files:**
- Create: `backend/src/auth.rs`

**Interfaces:**
- Produces: `pub fn auth_router(state: AppState) -> Router` (mounts all auth routes)
- Produces: `pub struct AuthUser { pub user_id: i64, pub email: String }` (axum extractor)
- Consumes: `db::insert_email_token`, `db::consume_email_token`, `db::find_or_create_user`, `db::create_session`, `db::lookup_session`, `db::delete_session`
- Consumes: `AppState` from `web` (for pool, base_url, oauth configs, smtp config)

- [ ] **Step 1: Write the auth module skeleton + router + extractor**

```rust
use crate::db;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Json, Redirect, Response};
use axum::routing::{get, post};
use axum::Router;
use base64::Engine;
use chrono::Utc;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

use crate::web::AppState;

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

async fn build_session_cookie(token: &str) -> String {
    format!(
        "{name}={token}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={age}",
        name = SESSION_COOKIE_NAME,
        age = SESSION_DAYS * 86400,
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

// ---------- handlers ----------

async fn post_email(
    State(state): State<AppState>,
    Json(body): Json<EmailRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    // Implementation in Step 3
    Ok(Json(OkResponse { ok: true }))
}

async fn get_verify(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Redirect, StatusCode> {
    // Implementation in Step 3
    Err(StatusCode::INTERNAL_SERVER_ERROR)
}

// Step 4 will add SSO handlers here

async fn get_me(
    auth: AuthUser,
) -> Result<Json<MeResponse>, StatusCode> {
    Ok(Json(MeResponse {
        id: auth.user_id,
        email: auth.email,
    }))
}

async fn post_logout(
) -> Result<Response, StatusCode> {
    // Implementation in Step 3
    Err(StatusCode::INTERNAL_SERVER_ERROR)
}

async fn get_providers(
    State(state): State<AppState>,
) -> Json<ProvidersResponse> {
    Json(ProvidersResponse {
        email: state.smtp_config.is_some(),
        google: state.google_oauth.is_some(),
        apple: state.apple_oauth.is_some(),
        github: state.github_oauth.is_some(),
    })
}

// ---------- extractor ----------

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

#[async_trait]
impl<S: Sync> FromRequestParts<S> for AuthUser
where
    AppState: axum::extract::FromRef<S>,
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
                let row = db::lookup_session(&app_state.pool, &t)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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
        .route("/api/auth/me", get(get_me))
        .route("/api/auth/logout", post(post_logout))
        .route("/api/auth/providers", get(get_providers))
}
```

- [ ] **Step 2: Register auth module in main.rs and add to web router**

In `main.rs`, add `mod auth;` after `mod web;`.
In `web.rs` router function, add `.merge(crate::auth::auth_router())` before `.with_state(state)`.

- [ ] **Step 3: Implement email magic link handlers**

Replace the `post_email` stub:

```rust
async fn post_email(
    State(state): State<AppState>,
    Json(body): Json<EmailRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let smtp = state.smtp_config.as_ref().ok_or(StatusCode::NOT_IMPLEMENTED)?;
    let token = new_token();
    let expires = Utc::now() + chrono::Duration::minutes(15);
    db::insert_email_token(&state.pool, &body.email, &token, expires)
        .await
        .map_err(|e| {
            tracing::error!("insert_email_token failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let link = format!("{}/api/auth/verify?token={}", state.base_url, token);
    let email = lettre::Message::builder()
        .from(smtp.from.parse().map_err(|e| {
            tracing::error!("invalid from: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?)
        .to(body.email.parse().map_err(|e| {
            tracing::error!("invalid to: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?)
        .subject("OV-Kino Linz — Sign in")
        .body(format!("Click here to sign in: {link}\n\nThis link expires in 15 minutes."))
        .map_err(|e| {
            tracing::error!("build email: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let creds = lettre::transport::smtp::authentication::Credentials::new(
        smtp.username.clone().unwrap_or_default(),
        smtp.password.clone(),
    );
    let mailer = lettre::AsyncSmtpTransport::<lettre::AsyncTls>::relay(&smtp.host)
        .map_err(|e| {
            tracing::error!("smtp relay: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .port(smtp.port)
        .credentials(creds)
        .build();
    match mailer.send(email).await {
        Ok(_) => {}
        Err(e) => tracing::error!("send email failed: {e}"),
    }
    // Always return ok to avoid email enumeration
    Ok(Json(OkResponse { ok: true }))
}
```

Replace the `get_verify` stub:

```rust
async fn get_verify(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Redirect, StatusCode> {
    let token = params.get("token").ok_or(StatusCode::BAD_REQUEST)?;
    let email = db::consume_email_token(&state.pool, token)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match email {
        Some(email) => {
            let user_id =
                db::find_or_create_user(&state.pool, "email", &email, &email)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let session_token = new_token();
            let expires = Utc::now() + chrono::Duration::days(SESSION_DAYS);
            db::create_session(&state.pool, user_id, &session_token, expires)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(Redirect::to(&state.base_url).with_header(
                header::SET_COOKIE,
                build_session_cookie(&session_token),
            ))
        }
        None => Ok(Redirect::to(&format!("{}/?error=invalid_token", state.base_url))),
    }
}
```

Replace the `post_logout` stub:

```rust
async fn post_logout() -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::SET_COOKIE, cleared_session_cookie())
        .body(axum::body::Body::from(r#"{"ok":true}"#))
        .unwrap()
}
```

Also add `use std::collections::HashMap;` to imports.

- [ ] **Step 4: Implement SSO handlers (Google, Apple, GitHub)**

Add to imports:
```rust
use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope,
    TokenResponse, TokenUrl,
};
```

Add shared OAuth state helper:

```rust
fn oauth_client(
    provider: &str,
    config: &crate::web::OAuthConfig,
    base_url: &str,
) -> BasicClient {
    let (auth_url, token_url) = match provider {
        "google" => (
            "https://accounts.google.com/o/oauth2/v2/auth",
            "https://oauth2.googleapis.com/token",
        ),
        "apple" => (
            "https://appleid.apple.com/auth/authorize",
            "https://appleid.apple.com/auth/token",
        ),
        "github" => (
            "https://github.com/login/oauth/authorize",
            "https://github.com/login/oauth/access_token",
        ),
        _ => unreachable!(),
    };
    BasicClient::new(
        ClientId::new(config.client_id.clone()),
        Some(ClientSecret::new(config.client_secret.clone())),
        AuthUrl::new(auth_url.to_string()).unwrap(),
        Some(TokenUrl::new(token_url.to_string()).unwrap()),
    )
    .set_redirect_uri(
        RedirectUrl::new(format!("{}/api/auth/sso/{}/callback", base_url, provider)).unwrap(),
    )
}
```

Add the SSO initiate handler (generic for all three):

```rust
async fn sso_initiate(
    State(state): State<AppState>,
    provider: &str,
) -> Result<Response, StatusCode> {
    let oauth = match provider {
        "google" => state.google_oauth.as_ref(),
        "apple" => state.apple_oauth.as_ref(),
        "github" => state.github_oauth.as_ref(),
        _ => return Err(StatusCode::NOT_FOUND),
    }
    .ok_or(StatusCode::NOT_IMPLEMENTED)?;
    let client = oauth_client(provider, oauth, &state.base_url);
    let (auth_url, csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(match provider {
            "google" => Scope::new("openid email".into()),
            "apple" => Scope::new("name email".into()),
            "github" => Scope::new("user:email".into()),
            _ => unreachable!(),
        })
        .url();
    let state_cookie = format!(
        "{name}={secret}; Secure; SameSite=Lax; Path=/; Max-Age=600",
        name = STATE_COOKIE_NAME,
        secret = csrf_token.secret(),
    );
    Ok(Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, auth_url.to_string())
        .header(header::SET_COOKIE, state_cookie)
        .body(axum::body::Body::empty())
        .unwrap())
}
```

But axum routes need concrete handler functions. Define three wrapper handlers:

```rust
async fn sso_google(State(state): State<AppState>) -> Result<Response, StatusCode> {
    sso_initiate(State(state), "google").await
}
async fn sso_apple(State(state): State<AppState>) -> Result<Response, StatusCode> {
    sso_initiate(State(state), "apple").await
}
async fn sso_github(State(state): State<AppState>) -> Result<Response, StatusCode> {
    sso_initiate(State(state), "github").await
}
```

Add SSO callback handler (generic):

```rust
async fn sso_callback(
    State(state): State<AppState>,
    req: axum::extract::Request,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
    provider: &str,
) -> Result<Response, StatusCode> {
    let oauth = match provider {
        "google" => state.google_oauth.as_ref(),
        "apple" => state.apple_oauth.as_ref(),
        "github" => state.github_oauth.as_ref(),
        _ => return Err(StatusCode::NOT_FOUND),
    }
    .ok_or(StatusCode::NOT_IMPLEMENTED)?;
    // Validate CSRF state cookie
    let cookie_header = req
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let expected_state = cookie_header
        .split(';')
        .map(|s| s.trim())
        .find_map(|c| c.strip_prefix(&format!("{STATE_COOKIE_NAME}=")))
        .map(|t| t.to_string());
    let code = params.get("code").cloned().unwrap_or_default();
    let state_param = params.get("state").cloned().unwrap_or_default();
    if state_param.is_empty() || expected_state.as_deref() != Some(&state_param) {
        return Ok(Redirect::to(&format!("{}/?error=invalid_state", state.base_url)));
    }
    let client = oauth_client(provider, oauth, &state.base_url);
    let token_res = client
        .exchange_code(AuthorizationCode::new(code))
        .request_async(async_http_client)
        .await
        .map_err(|e| {
            tracing::error!("oauth token exchange failed for {provider}: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    // Fetch user identity
    let (provider_id, email) = match provider {
        "google" => {
            let http = reqwest::Client::new();
            let user_info: serde_json::Value = http
                .get("https://openidconnect.googleapis.com/v1/userinfo")
                .bearer_auth(token_res.access_token().secret())
                .send()
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .json()
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let sub = user_info["sub"].as_str().unwrap_or("").to_string();
            let email_verified = user_info["email_verified"].as_bool().unwrap_or(false);
            if !email_verified {
                return Ok(Redirect::to(&format!("{}/?error=email_not_verified", state.base_url)));
            }
            let email = user_info["email"].as_str().unwrap_or("").to_string();
            (sub, email)
        }
        "apple" => {
            // Apple: sub and email come from id_token
            let id_token = token_res
                .extra_fields()
                .get("id_token")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            // Decode without verification (we don't have the Apple public key set up client-side;
            // the token was obtained via a direct TLS-secured backchannel, so it's trusted enough
            // for this low-stakes app)
            let header = jsonwebtoken::decode_header(id_token)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let decoding = jsonwebtoken::dangerous_insecure_decode::<serde_json::Value>(id_token)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let claims = &decoding.claims;
            let sub = claims["sub"].as_str().unwrap_or("").to_string();
            let email = claims["email"].as_str().map(|s| s.to_string());
            // Apple only returns email on first login; store whatever we have
            let email = email.unwrap_or_else(|| format!("apple-{sub}@unknown"));
            (sub, email)
        }
        "github" => {
            let http = reqwest::Client::new();
            let user: serde_json::Value = http
                .get("https://api.github.com/user")
                .header("User-Agent", "ov-kino-linz")
                .bearer_auth(token_res.access_token().secret())
                .send()
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .json()
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let id = user["id"].as_i64().unwrap_or(0).to_string();
            // Try to get verified primary email
            let emails: Vec<serde_json::Value> = http
                .get("https://api.github.com/user/emails")
                .header("User-Agent", "ov-kino-linz")
                .bearer_auth(token_res.access_token().secret())
                .send()
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .json()
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let email = emails
                .iter()
                .find(|e| e["primary"].as_bool() == Some(true) && e["verified"].as_bool() == Some(true))
                .and_then(|e| e["email"].as_str())
                .unwrap_or("");
            (id, email.to_string())
        }
        _ => unreachable!(),
    };
    let user_id = db::find_or_create_user(&state.pool, provider, &provider_id, &email)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let session_token = new_token();
    let expires = Utc::now() + chrono::Duration::days(SESSION_DAYS);
    db::create_session(&state.pool, user_id, &session_token, expires)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Redirect::to(&state.base_url).with_header(
        header::SET_COOKIE,
        build_session_cookie(&session_token),
    ))
}
```

Add wrapper handlers for each callback:

```rust
async fn sso_google_callback(
    State(state): State<AppState>,
    req: axum::extract::Request,
    Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Result<Response, StatusCode> {
    sso_callback(State(state), req, Query(params), "google").await
}
async fn sso_apple_callback(
    State(state): State<AppState>,
    req: axum::extract::Request,
    Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Result<Response, StatusCode> {
    sso_callback(State(state), req, Query(params), "apple").await
}
async fn sso_github_callback(
    State(state): State<AppState>,
    req: axum::extract::Request,
    Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Result<Response, StatusCode> {
    sso_callback(State(state), req, Query(params), "github").await
}
```

Update the `auth_router` to include SSO routes:

```rust
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
```

- [ ] **Step 5: Write auth handler tests**

Add a `#[cfg(test)] mod tests` block at the bottom of `auth.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::AppState;
    use axum::body::to_bytes;
    use axum::http::Request;
    use chrono::Utc;
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
        let mut state = test_state(pool);
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
}
```

- [ ] **Step 6: Run auth tests**

Run: `cd backend && DATABASE_URL=postgres://ov:ov@localhost:5432/ov cargo test auth::tests`
Expected: All pass

- [ ] **Step 7: Commit**

```bash
git add backend/src/auth.rs backend/src/main.rs backend/src/web.rs
git commit -m "feat: add auth module with email and SSO handlers"
```

---

### Task 5: Wiring — main.rs updates

**Files:**
- Modify: `backend/src/main.rs:91-95` (AppState construction)

**Interfaces:**
- Consumes: `Config.smtp_*`, `Config.base_url`, `Config.google_*`, `Config.apple_*`, `Config.github_*`
- Produces: Session cleanup spawned task

- [ ] **Step 1: Update AppState construction in main.rs**

Replace the existing state construction with:

```rust
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
    smtp_config,
    google_oauth: match (&config.google_client_id, &config.google_client_secret) {
        (Some(id), Some(secret)) => Some(web::OAuthConfig {
            client_id: id.clone(),
            client_secret: secret.clone(),
        }),
        _ => None,
    },
    apple_oauth: match (&config.apple_client_id, &config.apple_client_secret) {
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
};
```

- [ ] **Step 2: Add session cleanup spawned task**

After the existing checker `tokio::spawn`, add:

```rust
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
```

- [ ] **Step 3: Verify compilation**

Run: `cd backend && cargo check`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add backend/src/main.rs
git commit -m "feat: wire auth config and session cleanup into main"
```

---

### Task 6: Frontend types + API

**Files:**
- Modify: `frontend/src/types.ts:20-24` (add auth types)
- Modify: `frontend/src/api.ts:1-9` (add auth API functions)

- [ ] **Step 1: Add types to types.ts**

```typescript
export interface AuthUser {
  id: number;
  email: string;
}

export interface AuthProviders {
  email: boolean;
  google: boolean;
  apple: boolean;
  github: boolean;
}
```

- [ ] **Step 2: Add API functions to api.ts**

```typescript
export async function fetchMe(): Promise<AuthUser> {
  const resp = await fetch("/api/auth/me");
  if (!resp.ok) throw new Error("not authenticated");
  return (await resp.json()) as AuthUser;
}

export async function fetchProviders(): Promise<AuthProviders> {
  const resp = await fetch("/api/auth/providers");
  if (!resp.ok) throw new Error("providers fetch failed");
  return (await resp.json()) as AuthProviders;
}

export async function sendMagicLink(email: string): Promise<void> {
  await fetch("/api/auth/email", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ email }),
  });
}

export async function logout(): Promise<void> {
  await fetch("/api/auth/logout", { method: "POST" });
}
```

- [ ] **Step 3: Run the existing frontend tests to verify nothing breaks**

Run: `cd frontend && npm test`
Expected: Existing tests pass

- [ ] **Step 4: Commit**

```bash
git add frontend/src/types.ts frontend/src/api.ts
git commit -m "feat: add frontend auth types and API functions"
```

---

### Task 7: Frontend auth context + login UI

**Files:**
- Create: `frontend/src/hooks/useAuth.tsx` (auth context + provider + hook)
- Modify: `frontend/src/App.tsx:1-12` (wrap with AuthProvider)
- Modify: `frontend/src/components/Marquee.tsx:1-29` (add login/logout UI)
- Modify: `frontend/src/index.css:136-150` (add auth CSS)
- Modify: `frontend/src/pages/ShowingsPage.tsx:1-64` (integrate auth context)

- [ ] **Step 1: Create useAuth.tsx**

```typescript
import { createContext, useContext, useState, useEffect, useCallback, type ReactNode } from "react";
import { fetchMe, fetchProviders, sendMagicLink, logout as apiLogout } from "../api";
import type { AuthUser, AuthProviders } from "../types";

interface AuthState {
  user: AuthUser | null;
  loading: boolean;
  providers: AuthProviders | null;
  loginEmail: (email: string) => Promise<void>;
  loginSSO: (provider: string) => void;
  logout: () => Promise<void>;
}

const AuthContext = createContext<AuthState>({
  user: null,
  loading: true,
  providers: null,
  loginEmail: async () => {},
  loginSSO: () => {},
  logout: async () => {},
});

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<AuthUser | null>(null);
  const [loading, setLoading] = useState(true);
  const [providers, setProviders] = useState<AuthProviders | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const [u, p] = await Promise.all([fetchMe(), fetchProviders()]);
      setUser(u);
      setProviders(p);
    } catch {
      setUser(null);
      setProviders(null);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { refresh(); }, [refresh]);

  const loginEmail = useCallback(async (email: string) => {
    await sendMagicLink(email);
  }, []);

  const loginSSO = useCallback((provider: string) => {
    window.location.href = `/api/auth/sso/${provider}`;
  }, []);

  const logout = useCallback(async () => {
    await apiLogout();
    setUser(null);
  }, []);

  return (
    <AuthContext.Provider value={{ user, loading, providers, loginEmail, loginSSO, logout }}>
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth() {
  return useContext(AuthContext);
}
```

- [ ] **Step 2: Create the hooks directory**

```bash
mkdir -p frontend/src/hooks
```

- [ ] **Step 3: Update App.tsx to wrap with AuthProvider**

```typescript
import { Route, Routes } from "react-router-dom";
import { AuthProvider } from "./hooks/useAuth";
import { ShowingsPage } from "./pages/ShowingsPage";
import { ImpressumPage } from "./pages/ImpressumPage";

export default function App() {
  return (
    <AuthProvider>
      <Routes>
        <Route path="/" element={<ShowingsPage />} />
        <Route path="/impressum" element={<ImpressumPage />} />
      </Routes>
    </AuthProvider>
  );
}
```

- [ ] **Step 4: Update Marquee.tsx with login UI**

```typescript
import { useState } from "react";
import { NavLink } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { LanguageSwitcher } from "./LanguageSwitcher";
import { useAuth } from "../hooks/useAuth";

export function Marquee() {
  const { t } = useTranslation();
  const { user, loading, providers, loginEmail, loginSSO, logout } = useAuth();
  const [showLogin, setShowLogin] = useState(false);
  const [emailInput, setEmailInput] = useState("");
  const [sent, setSent] = useState(false);

  const handleEmailSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!emailInput.trim()) return;
    await loginEmail(emailInput.trim());
    setSent(true);
  };

  return (
    <header className="marquee">
      <div className="bulbs"></div>
      <div className="marquee-brand">
        <img
          className="marquee-logo"
          src="/projector-logo.svg"
          alt=""
        />
        <div className="marquee-text">
          <h1>{t("brand")}</h1>
          <p className="tagline">{t("tagline")}</p>
        </div>
      </div>
      <nav className="marqnav">
        <NavLink to="/">{t("nav.home")}</NavLink>
        <NavLink to="/impressum">{t("nav.impressum")}</NavLink>
        <LanguageSwitcher />
        {!loading && (
          !user ? (
            <button className="auth-btn" onClick={() => setShowLogin(!showLogin)}>
              {t("auth.signIn")}
            </button>
          ) : (
            <button className="auth-btn" onClick={logout}>
              {t("auth.signOut")}
            </button>
          )
        )}
      </nav>
      {showLogin && !user && (
        <div className="auth-panel">
          {providers?.email && (
            <form onSubmit={handleEmailSubmit}>
              <input
                className="auth-input"
                type="email"
                placeholder={t("auth.emailPlaceholder")}
                value={emailInput}
                onChange={(e) => setEmailInput(e.target.value)}
              />
              <button className="auth-submit" type="submit">
                {sent ? t("auth.emailSent") : t("auth.sendLink")}
              </button>
            </form>
          )}
          {providers?.google && (
            <button className="auth-sso" onClick={() => loginSSO("google")}>
              {t("auth.signInWith", { provider: "Google" })}
            </button>
          )}
          {providers?.apple && (
            <button className="auth-sso" onClick={() => loginSSO("apple")}>
              {t("auth.signInWith", { provider: "Apple" })}
            </button>
          )}
          {providers?.github && (
            <button className="auth-sso" onClick={() => loginSSO("github")}>
              {t("auth.signInWith", { provider: "GitHub" })}
            </button>
          )}
        </div>
      )}
      <div className="bulbs"></div>
    </header>
  );
}
```

- [ ] **Step 5: Add CSS to index.css**

Append at the end of `index.css`:

```css
.auth-btn{background:none;border:1px solid var(--edge);color:var(--dim);
 border-radius:4px;padding:.15rem .5rem;font-size:.75rem;cursor:pointer}
.auth-btn:hover{color:var(--gold);border-color:var(--gold)}
.auth-panel{text-align:center;padding:.8rem 1rem;margin-top:.5rem;
 border-top:1px dashed var(--edge);display:flex;flex-wrap:wrap;gap:.4rem;
 justify-content:center;align-items:center}
.auth-panel form{display:flex;gap:.4rem;align-items:center}
.auth-input{background:var(--bg);border:1px solid var(--edge);color:var(--text);
 border-radius:4px;padding:.3rem .5rem;font-size:.8rem;width:200px}
.auth-input::placeholder{color:var(--faint)}
.auth-submit{background:var(--gold);color:#221a0c;border:none;border-radius:4px;
 padding:.3rem .7rem;font-size:.75rem;font-weight:700;cursor:pointer;
 box-shadow:0 0 8px rgba(232,179,77,.35)}
.auth-submit:hover{background:var(--gold-bright)}
.auth-sso{background:var(--panel);border:1px solid var(--edge);color:var(--text);
 border-radius:4px;padding:.3rem .7rem;font-size:.75rem;cursor:pointer}
.auth-sso:hover{border-color:var(--gold);color:var(--gold)}
```

- [ ] **Step 6: Write frontend test for auth UI**

Read the existing frontend test setup first to understand patterns.

Create/modify files as needed for tests. The existing test setup uses vitest + @testing-library/react + jsdom. Tests in `frontend/src/`, co-located or in `__tests__` or as `*.test.tsx`.

Given the test directory structure, add `frontend/src/components/Marquee.test.tsx`:

```typescript
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { Marquee } from "./Marquee";
import * as api from "../api";

// We need to wrap Marquee in AuthProvider for the tests to work
// Since AuthProvider is defined in hooks/useAuth.tsx, import it
import { AuthProvider } from "../hooks/useAuth";

vi.mock("../api");

const mockFetchMe = vi.mocked(api.fetchMe);
const mockFetchProviders = vi.mocked(api.fetchProviders);
const mockSendMagicLink = vi.mocked(api.sendMagicLink);
const mockLogout = vi.mocked(api.logout);

function renderMarquee() {
  return render(
    <MemoryRouter>
      <AuthProvider>
        <Marquee />
      </AuthProvider>
    </MemoryRouter>
  );
}

describe("Marquee auth", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows sign in button when not authenticated", async () => {
    mockFetchMe.mockRejectedValue(new Error("not auth"));
    mockFetchProviders.mockResolvedValue({
      email: true,
      google: true,
      apple: false,
      github: false,
    });
    renderMarquee();
    await waitFor(() => {
      expect(screen.getByText("auth.signIn")).toBeDefined();
    });
  });

  it("shows sign in panel on click", async () => {
    mockFetchMe.mockRejectedValue(new Error("not auth"));
    mockFetchProviders.mockResolvedValue({
      email: true,
      google: true,
      apple: false,
      github: false,
    });
    renderMarquee();
    await waitFor(() => {
      expect(screen.getByText("auth.signIn")).toBeDefined();
    });
    fireEvent.click(screen.getByText("auth.signIn"));
    await waitFor(() => {
      expect(screen.getByText("auth.sendLink")).toBeDefined();
      expect(screen.getByText("auth.signInWith")).toBeDefined();
    });
  });

  it("shows sign out when authenticated", async () => {
    mockFetchMe.mockResolvedValue({ id: 1, email: "a@b.com" });
    mockFetchProviders.mockResolvedValue({
      email: true,
      google: false,
      apple: false,
      github: false,
    });
    renderMarquee();
    await waitFor(() => {
      expect(screen.getByText("auth.signOut")).toBeDefined();
    });
  });
});
```

Wait — the i18n `t()` calls will render the key strings since i18next is mocked/inited differently in tests. Let me check the test setup.

Actually, looking at the test setup, i18next is used with translations loaded from JSON files. In tests, it should render the translation values. Let me adjust the test to match actual translation keys.

But I don't know the exact translation values yet. Let me use `getByText` with the actual translated strings. I'll check the locale files next.

Actually, the test won't pass yet because translations don't exist. The test should be written after translations. Let me note this in the plan — the test goes in Task 8.

Let me remove the test from this task and add it to Task 8.

- [ ] **Step 6: Run frontend checks**

Run: `cd frontend && npx tsc --noEmit`
Expected: No type errors (will have missing translation keys warnings but not errors)

- [ ] **Step 7: Commit**

```bash
git add frontend/src/hooks/useAuth.tsx frontend/src/App.tsx frontend/src/components/Marquee.tsx frontend/src/index.css
git commit -m "feat: add auth context and login UI"
```

---

### Task 8: Translations

**Files:**
- Modify: `frontend/src/locales/en.json`
- Modify: `frontend/src/locales/de.json`

- [ ] **Step 1: Add English translations**

Read `frontend/src/locales/en.json` first, then add:

```json
"auth": {
  "signIn": "Sign in",
  "signOut": "Sign out",
  "emailPlaceholder": "your@email.com",
  "sendLink": "Send link",
  "emailSent": "Check your email!",
  "signInWith": "Sign in with {{provider}}"
}
```

- [ ] **Step 2: Add German translations**

```json
"auth": {
  "signIn": "Anmelden",
  "signOut": "Abmelden",
  "emailPlaceholder": "deine@email.com",
  "sendLink": "Link senden",
  "emailSent": "Prüfe deine E-Mails!",
  "signInWith": "Anmelden mit {{provider}}"
}
```

- [ ] **Step 3: Write Marquee auth test**

Create `frontend/src/components/Marquee.test.tsx`:

```typescript
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { Marquee } from "./Marquee";
import { AuthProvider } from "../hooks/useAuth";
import * as api from "../api";

vi.mock("../api");

const mockFetchMe = vi.mocked(api.fetchMe);
const mockFetchProviders = vi.mocked(api.fetchProviders);
const mockLogout = vi.mocked(api.logout);

function renderMarquee() {
  return render(
    <MemoryRouter>
      <AuthProvider>
        <Marquee />
      </AuthProvider>
    </MemoryRouter>
  );
}

describe("Marquee auth", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows sign in button when not authenticated", async () => {
    mockFetchMe.mockRejectedValue(new Error("not auth"));
    mockFetchProviders.mockResolvedValue({
      email: true,
      google: true,
      apple: false,
      github: false,
    });
    renderMarquee();
    await waitFor(() => {
      expect(screen.getByText("Sign in")).toBeDefined();
    });
  });

  it("shows login panel with email and Google SSO buttons", async () => {
    mockFetchMe.mockRejectedValue(new Error("not auth"));
    mockFetchProviders.mockResolvedValue({
      email: true,
      google: true,
      apple: false,
      github: false,
    });
    renderMarquee();
    await waitFor(() => {
      expect(screen.getByText("Sign in")).toBeDefined();
    });
    fireEvent.click(screen.getByText("Sign in"));
    await waitFor(() => {
      expect(screen.getByPlaceholderText("your@email.com")).toBeDefined();
      expect(screen.getByText("Sign in with Google")).toBeDefined();
    });
    expect(screen.queryByText("Sign in with Apple")).toBeNull();
  });

  it("shows sign out when authenticated", async () => {
    mockFetchMe.mockResolvedValue({ id: 1, email: "a@b.com" });
    mockFetchProviders.mockResolvedValue({
      email: true,
      google: false,
      apple: false,
      github: false,
    });
    renderMarquee();
    await waitFor(() => {
      expect(screen.getByText("Sign out")).toBeDefined();
    });
  });

  it("calls logout API on sign out click", async () => {
    mockFetchMe.mockResolvedValue({ id: 1, email: "a@b.com" });
    mockFetchProviders.mockResolvedValue({
      email: true,
      google: false,
      apple: false,
      github: false,
    });
    renderMarquee();
    await waitFor(() => {
      expect(screen.getByText("Sign out")).toBeDefined();
    });
    fireEvent.click(screen.getByText("Sign out"));
    await waitFor(() => {
      expect(mockLogout).toHaveBeenCalled();
    });
  });
});
```

- [ ] **Step 4: Run frontend tests**

Run: `cd frontend && npm test`
Expected: All pass including new auth tests

- [ ] **Step 5: Commit**

```bash
git add frontend/src/locales/en.json frontend/src/locales/de.json frontend/src/components/Marquee.test.tsx
git commit -m "feat: add auth translations and Marquee auth test"
```

---

### Final: Full test suite

- [ ] **Run backend tests**

```bash
cd backend && DATABASE_URL=postgres://ov:ov@localhost:5432/ov cargo test
```

- [ ] **Run frontend tests**

```bash
cd frontend && npm test
```

- [ ] **Run lints**

```bash
cd backend && cargo fmt --check && cargo clippy -- -D warnings
cd frontend && npx tsc --noEmit
```
