# Dev Fake Login Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a dev-only fake-login so a local box without SMTP/SSO providers can sign in: a `FAKE_LOGIN` env-gated backend endpoint mints a real session (user `dev@ov.local`), and the login modal shows a "Dev: sign in as dev@ov.local" button when the backend reports it's enabled.

**Architecture:** Backend: `Config.fake_login` flag → `AppState.fake_login` → a `GET /api/auth/dev-login` handler reusing `find_or_create_user`/`create_session`/`ov_session` cookie, redirecting to host-relative `/`; `providers.dev` exposes the flag to the SPA. Frontend: `AuthProviders.dev` drives a dev button in `LoginModal`. Prod never sets the flag.

**Tech Stack:** Rust/axum + sqlx (backend), React 19 + react-i18next (frontend), docker-compose (dev stack).

## Global Constraints

- The dev-login endpoint MUST 404 whenever `FAKE_LOGIN` is not explicitly `1`/`true` — it must be impossible to enable from production configs (the Helm chart is untouched).
- No comments in code.
- Follow existing patterns: named handlers, `State(state)` extraction, `serde(rename_all = "camelCase")`, locale keys in both `en.json` and `de.json`.
- Fixed dev identity: provider `"dev"`, provider_id + email `"dev@ov.local"` (`DEV_EMAIL` const).
- Dev-login redirect target is `/` (host-relative), NOT `BASE_URL`.
- Tests: backend `cargo test` (needs `DATABASE_URL` set — Postgres running), frontend `npm test` and `npm run build` from `frontend/`.

---

### Task 1: `FAKE_LOGIN` config flag and AppState plumbing

**Files:**
- Modify: `backend/src/config.rs` (`Config` struct + `from_lookup` + `mod tests`)
- Modify: `backend/src/web.rs` (`AppState` struct; test constructions in `api_showings_three_states`, `ics_route_renders_events`, `poster_route_serves_and_guards`, `healthz_route`)
- Modify: `backend/src/main.rs` (`AppState` construction)
- Modify: `backend/src/auth.rs` (`test_state` helper)

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `Config { ..., pub fake_login: bool }` (from env `FAKE_LOGIN`).
  - `AppState { ..., pub fake_login: bool }`.
  - `auth::test_state(pool)` returns `fake_login: false` (all `AppState` sites compile).

- [ ] **Step 1: Write the failing config tests**

Add to `mod tests` in `backend/src/config.rs`:

```rust
#[test]
fn fake_login_defaults_to_false() {
    let cfg = Config::from_lookup(env_of(&[("DATABASE_URL", "postgres://x")])).unwrap();
    assert!(!cfg.fake_login);
}

#[test]
fn fake_login_parses_enabled_values() {
    for v in ["1", "true", "TRUE"] {
        let cfg = Config::from_lookup(env_of(&[
            ("DATABASE_URL", "postgres://x"),
            ("FAKE_LOGIN", v),
        ]))
        .unwrap();
        assert!(cfg.fake_login, "expected FAKE_LOGIN={v} to enable dev login");
    }
}

#[test]
fn fake_login_parses_disabled_values() {
    for v in ["0", "false", "yes", ""] {
        let cfg = Config::from_lookup(env_of(&[
            ("DATABASE_URL", "postgres://x"),
            ("FAKE_LOGIN", v),
        ]))
        .unwrap();
        assert!(!cfg.fake_login, "expected FAKE_LOGIN={v:?} to disable dev login");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd backend && cargo test --lib fake_login`
Expected: FAIL — compile error, `Config` has no field `fake_login`.

- [ ] **Step 3: Implement config, state, and all construction sites**

`backend/src/config.rs` — add the field to `Config` (after `github_client_secret`):

```rust
    pub github_client_id: Option<String>,
    pub github_client_secret: Option<String>,
    pub fake_login: bool,
```

In `from_lookup`, add to the returned `Ok(Config { ... })`:

```rust
            github_client_secret: get("GITHUB_CLIENT_SECRET"),
            fake_login: get("FAKE_LOGIN")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
```

`backend/src/web.rs` — add to `AppState` (after `base_url`):

```rust
    pub base_url: String,
    pub fake_login: bool,
```

Update every `AppState { ... }` literal in `backend/src/web.rs` (`mod tests`): add `fake_login: false,` after `base_url: ...`.

`backend/src/main.rs` — in the `web::AppState { ... }` construction, after `base_url: config.base_url.clone(),` add:

```rust
        fake_login: config.fake_login,
```

`backend/src/auth.rs` — in `fn test_state`, after `base_url: "http://localhost:8080".into(),` add:

```rust
            fake_login: false,
```

- [ ] **Step 4: Run the config tests to verify they pass**

Run: `cd backend && cargo test --lib fake_login`
Expected: PASS (3 tests).

- [ ] **Step 5: Run the full backend suite**

Run: `cd backend && cargo test` (requires `DATABASE_URL=postgres://ov:ov@localhost:5432/ov` and Postgres up)
Expected: PASS — all existing tests compile and pass (proves every `AppState` site was updated).

- [ ] **Step 6: Commit**

```bash
git add backend/src/config.rs backend/src/web.rs backend/src/main.rs backend/src/auth.rs
git commit -m "feat: FAKE_LOGIN config flag and AppState plumbing"
```

---

### Task 2: dev-login endpoint + `providers.dev`

**Files:**
- Modify: `backend/src/auth.rs` (`ProvidersResponse`, `get_providers`, new `get_dev_login` handler, router, `mod tests`)

**Interfaces:**
- Consumes: `Config.fake_login` → `AppState.fake_login` (Task 1); `db::find_or_create_user`, `db::create_session`, `new_token`, `build_session_cookie`, `redirect_to_with_cookie`, `SESSION_DAYS` (all existing in `auth.rs`).
- Produces:
  - `GET /api/auth/dev-login` — 404 unless `AppState.fake_login`; else 302 → `/` with `Set-Cookie: ov_session=...`.
  - `GET /api/auth/providers` now returns `{ email, google, github, dev }`.
  - Helper `test_state_dev(pool) -> AppState` (fake_login: true) in `mod tests`.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `backend/src/auth.rs`. First a helper next to `test_state`:

```rust
    fn test_state_dev(pool: PgPool) -> AppState {
        let mut state = test_state(pool);
        state.fake_login = true;
        state
    }
```

Extend the existing `providers_endpoint` test to also assert `dev`:

```rust
        assert_eq!(json["email"], false);
        assert_eq!(json["google"], false);
        assert_eq!(json["github"], false);
        assert_eq!(json["dev"], false);
```

Add these new tests:

```rust
    #[sqlx::test(migrations = "./migrations")]
    async fn providers_endpoint_reports_dev_login(pool: PgPool) {
        let state = test_state_dev(pool);
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
        assert_eq!(json["dev"], true);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn dev_login_disabled_returns_404(pool: PgPool) {
        let state = test_state(pool);
        let app = Router::new().merge(auth_router()).with_state(state);
        let resp = app
            .oneshot(
                Request::get("/api/auth/dev-login")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 404);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn dev_login_creates_session_and_redirects(pool: PgPool) {
        let state = test_state_dev(pool.clone());
        let app = Router::new().merge(auth_router()).with_state(state);
        let resp = app
            .oneshot(
                Request::get("/api/auth/dev-login")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 302);
        assert_eq!(
            resp.headers().get("location").unwrap().to_str().unwrap(),
            "/"
        );
        let cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
        let token = cookie
            .strip_prefix("ov_session=")
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();

        // the minted session authenticates /api/auth/me
        let state = test_state(pool);
        let app = Router::new().merge(auth_router()).with_state(state);
        let resp = app
            .oneshot(
                Request::get("/api/auth/me")
                    .header("Cookie", format!("ov_session={token}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["email"], "dev@ov.local");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd backend && cargo test --lib dev_login && cargo test --lib providers`
Expected: FAIL — `providers_endpoint` asserts `dev` but the response has no `dev` key; `dev_login_*` fail (route not registered / no handler). The handler code does not exist yet.

- [ ] **Step 3: Implement**

In `backend/src/auth.rs`:

Add the `dev` field to `ProvidersResponse`:

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProvidersResponse {
    email: bool,
    google: bool,
    github: bool,
    dev: bool,
}
```

Add a const near the other cookie/const definitions:

```rust
const DEV_EMAIL: &str = "dev@ov.local";
```

Add the handler (place it after `post_logout`, before `get_providers`):

```rust
async fn get_dev_login(State(state): State<AppState>) -> Result<Response, StatusCode> {
    if !state.fake_login {
        return Err(StatusCode::NOT_FOUND);
    }
    let user_id = db::find_or_create_user(&state.pool, "dev", DEV_EMAIL, DEV_EMAIL)
        .await
        .map_err(|e| {
            tracing::error!("dev login: find_or_create_user failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let session_token = new_token();
    let expires = Utc::now() + chrono::Duration::days(SESSION_DAYS);
    db::create_session(&state.pool, user_id, &session_token, expires)
        .await
        .map_err(|e| {
            tracing::error!("dev login: create_session failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(redirect_to_with_cookie(
        "/",
        &build_session_cookie(&session_token).await,
    ))
}
```

Update `get_providers`:

```rust
async fn get_providers(State(state): State<AppState>) -> Json<ProvidersResponse> {
    Json(ProvidersResponse {
        email: state.smtp_config.is_some(),
        google: state.google_oauth.is_some(),
        github: state.github_oauth.is_some(),
        dev: state.fake_login,
    })
}
```

Register the route in `auth_router()` (after the `sso/github/callback` route):

```rust
        .route("/api/auth/dev-login", get(get_dev_login))
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd backend && cargo test --lib dev_login && cargo test --lib providers`
Expected: PASS — all 4 tests (disabled 404, creates-session redirect + me, providers dev false/true).

- [ ] **Step 5: Run the full backend suite**

Run: `cd backend && cargo test` (needs `DATABASE_URL` + Postgres)
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add backend/src/auth.rs
git commit -m "feat: dev-login endpoint mints a real session"
```

---

### Task 3: Frontend dev sign-in button

**Files:**
- Modify: `frontend/src/types.ts` (`AuthProviders`)
- Modify: `frontend/src/locales/en.json`, `frontend/src/locales/de.json` (`auth.devLogin`)
- Modify: `frontend/src/components/LoginModal.tsx`
- Modify: `frontend/src/components/LoginModal.test.tsx`
- Modify: `frontend/src/App.test.tsx`, `frontend/src/pages/PreferencesPage.test.tsx` (provider mock bodies gain `dev: false`)

**Interfaces:**
- Consumes: `providers.dev: boolean` from the backend (Task 2, `AuthProviders`); locale key `auth.devLogin`.
- Produces: a `Dev: sign in as dev@ov.local` button in the login modal, rendered only when `providers.dev` is truthy, navigating to `/api/auth/dev-login` on click.

- [ ] **Step 1: Write the failing tests**

In `frontend/src/components/LoginModal.test.tsx`:

- In `beforeEach`, the provider mock gains `dev: false`:
  ```ts
  mockFetchProviders.mockResolvedValue({
    email: true,
    google: true,
    github: true,
    dev: false,
  });
  ```
- In `afterEach`, add `vi.unstubAllGlobals();`.
- Add the `auth.devLogin` button text to the import? Not needed — use the literal string "Dev: sign in as dev@ov.local".

Add three new tests inside the existing `describe("LoginModal", ...)` block:

```tsx
it("shows the dev login button only when the backend enables it", async () => {
  mockFetchProviders.mockResolvedValue({
    email: true,
    google: true,
    github: true,
    dev: true,
  });
  renderModal();
  await act(async () => {});
  expect(screen.getByText("Dev: sign in as dev@ov.local")).toBeInTheDocument();
});

it("hides the dev login button when dev login is disabled", async () => {
  mockFetchProviders.mockResolvedValue({
    email: true,
    google: true,
    github: true,
    dev: false,
  });
  renderModal();
  await act(async () => {});
  expect(screen.queryByText("Dev: sign in as dev@ov.local")).toBeNull();
});

it("navigates to the dev-login endpoint when clicked", async () => {
  const locationStub = { href: "" };
  vi.stubGlobal("location", locationStub);
  mockFetchProviders.mockResolvedValue({
    email: true,
    google: true,
    github: true,
    dev: true,
  });
  renderModal();
  await act(async () => {});
  fireEvent.click(screen.getByText("Dev: sign in as dev@ov.local"));
  expect(locationStub.href).toBe("/api/auth/dev-login");
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npm test`
Expected: FAIL — type error (the mock now returns a `dev` field `AuthProviders` doesn't have) and/or the button is not rendered.

- [ ] **Step 3: Implement**

`frontend/src/types.ts` — `AuthProviders` gains the field:

```ts
export interface AuthProviders {
  email: boolean;
  google: boolean;
  github: boolean;
  dev: boolean;
}
```

`frontend/src/locales/en.json` — inside `auth`, add:

```json
    "devLogin": "Dev: sign in as dev@ov.local",
```

`frontend/src/locales/de.json` — inside `auth`, add:

```json
    "devLogin": "Dev: als dev@ov.local anmelden",
```

`frontend/src/components/LoginModal.tsx` — after the GitHub provider button, inside the modal, add:

```tsx
        {providers?.dev && (
          <button
            className="auth-provider"
            onClick={() => {
              window.location.href = "/api/auth/dev-login";
            }}
          >
            <span>{t("auth.devLogin")}</span>
          </button>
        )}
```

`frontend/src/App.test.tsx` — in `mockFetch`, the providers branch body gains `dev: false`:

```tsx
        return { ok: true, json: async () => ({ email: true, google: true, github: true, dev: false }) };
```

`frontend/src/pages/PreferencesPage.test.tsx` — in `mockAuthFetch`, the providers branch body gains `dev: false`:

```tsx
        return { ok: true, json: async () => ({ email: true, google: true, github: true, dev: false }) };
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd frontend && npm test`
Expected: PASS — 3 new LoginModal tests green, full suite green.

- [ ] **Step 5: Typecheck and build**

Run: `cd frontend && npm run build`
Expected: build succeeds.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/types.ts frontend/src/locales/en.json frontend/src/locales/de.json frontend/src/components/LoginModal.tsx frontend/src/components/LoginModal.test.tsx frontend/src/App.test.tsx frontend/src/pages/PreferencesPage.test.tsx
git commit -m "feat: dev sign-in button in login modal"
```

---

### Task 4: Dev-stack enablement and docs

**Files:**
- Modify: `docker-compose.dev.yml`
- Modify: `LOCAL_DEV.md`

**Interfaces:**
- Consumes: the `FAKE_LOGIN` env var (Task 1).
- Produces: fake login on by default in the dev compose stack; a local-dev usage guide in `LOCAL_DEV.md`.

- [ ] **Step 1: Enable the flag in the dev compose stack**

In `docker-compose.dev.yml`, under the `backend` service `environment`, add after `TELEGRAM_CHAT_ID`:

```yaml
      FAKE_LOGIN: ${FAKE_LOGIN:-1}
```

- [ ] **Step 2: Document local usage**

In `LOCAL_DEV.md`, after the "Optional: Telegram notifications locally" section, add a new section:

~~~markdown
## Fake login (local development)

No SMTP or SSO providers are configured on a local box, so the login modal has
no working options. A dev-only endpoint mints a real session for a fixed dev
user:

```bash
export FAKE_LOGIN=1    # dev compose stack defaults to 1 already
```

Restart the backend, open the app, click **Sign in**, then
**Dev: sign in as dev@ov.local**. The backend creates the `dev@ov.local` user
on first use and sets the normal `ov_session` cookie (30 days).

- The login modal only shows the dev button when the backend reports it
  (`GET /api/auth/providers` → `dev: true`).
- `FAKE_LOGIN=0` or leaving it unset disables the endpoint (returns 404).
  Production never sets it.
- The dev-login redirects to `/` on the same host, so it works from both the
  Vite dev server (:5173) and the backend-served SPA (:8080).
~~~

- [ ] **Step 3: Verify the compose file is valid**

Run: `docker compose -f docker-compose.dev.yml config >/dev/null && echo OK`
Expected: prints `OK`. If docker is unavailable, skip this step and note it in the report.

- [ ] **Step 4: Commit**

```bash
git add docker-compose.dev.yml LOCAL_DEV.md
git commit -m "chore: enable fake login in dev compose and document it"
```
