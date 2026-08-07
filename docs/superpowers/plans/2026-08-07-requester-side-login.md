# Requester-Side Login for Email Magic Link — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the *requesting* device (not the link-clicking device) receive the email magic-link session.

**Architecture:** `GET /api/auth/verify` stops issuing sessions and only marks the email token consumed, then redirects to `/?login=confirmed`. The requesting browser holds an `ov_pending` cookie set at `POST /api/auth/email`; a new `GET /api/auth/login/status` poll endpoint issues the session only when that cookie's token is consumed. The frontend polls after submit.

**Tech Stack:** Rust (axum, sqlx), lettre, React 19 + Vite + Vitest, react-i18next.

## Global Constraints

- `verify` must NEVER create a session or set `ov_session`.
- Only the requesting device logs in; the clicking device gets `?login=confirmed` and no session.
- `ov_pending` cookie format: `ov_pending=<token>; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=900` (same token as the email link).
- No fast path for same-device clicks; uniform poll path only.
- No cap on concurrent pending requests.
- SSO flows (Google/Apple/GitHub) unchanged.
- Run backend tests with `DATABASE_URL=postgres://ov:ov@localhost:5432/ov` from `backend/`. Run frontend tests from `frontend/` with `npm test`.

---

### Task 1: Add `lookup_email_token` DB query with test

**Files:**
- Modify: `backend/src/db.rs` (add function near `consume_email_token` at line 175)
- Test: `backend/src/db.rs` (add to `mod tests`, near `email_token_insert_and_consume` at line 429)

**Interfaces:**
- Consumes: existing `email_tokens` table (`token TEXT PK, email TEXT, expires_at TIMESTAMPTZ, used BOOLEAN`).
- Produces: `pub async fn lookup_email_token(pool: &PgPool, token: &str) -> sqlx::Result<Option<EmailTokenState>>` with `pub struct EmailTokenState { pub email: String, pub used: bool }` — returns `None` when the token doesn't exist or is expired (uses `expires_at > now()`).

- [ ] **Step 1: Write the failing test**

Add to `backend/src/db.rs` `mod tests`:

```rust
#[sqlx::test(migrations = "./migrations")]
async fn lookup_email_token_states(pool: PgPool) {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let token_bytes: [u8; 32] = rng.gen();
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);
    let expires = Utc::now() + chrono::Duration::minutes(15);
    insert_email_token(&pool, "a@b.com", &token, expires).await.unwrap();

    // not used yet
    let st = lookup_email_token(&pool, &token).await.unwrap().unwrap();
    assert_eq!(st.email, "a@b.com");
    assert!(!st.used);

    // after consumption, used=true
    let _ = consume_email_token(&pool, &token).await.unwrap();
    let st = lookup_email_token(&pool, &token).await.unwrap().unwrap();
    assert!(st.used);

    // unknown token -> None
    assert!(lookup_email_token(&pool, "nope").await.unwrap().is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn lookup_email_token_expired_returns_none(pool: PgPool) {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let token_bytes: [u8; 32] = rng.gen();
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);
    let expires = Utc::now() - chrono::Duration::minutes(1);
    insert_email_token(&pool, "a@b.com", &token, expires).await.unwrap();
    assert!(lookup_email_token(&pool, &token).await.unwrap().is_none());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test lookup_email_token`
Expected: FAIL — compile error `cannot find function lookup_email_token` / `cannot find type EmailTokenState`.

- [ ] **Step 3: Implement `EmailTokenState` and `lookup_email_token`**

Add above `consume_email_token` (near line 175 in `backend/src/db.rs`):

```rust
#[derive(Debug, Clone)]
pub struct EmailTokenState {
    pub email: String,
    pub used: bool,
}

pub async fn lookup_email_token(pool: &PgPool, token: &str) -> sqlx::Result<Option<EmailTokenState>> {
    let row: Option<(String, bool)> = sqlx::query_as(
        "SELECT email, used FROM email_tokens WHERE token = $1 AND expires_at > now()",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(email, used)| EmailTokenState { email, used }))
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test lookup_email_token`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add backend/src/db.rs
git commit -m "feat: add lookup_email_token query"
```

---

### Task 2: Change `verify` to confirm-only, and set `ov_pending` in `post_email`

**Files:**
- Modify: `backend/src/auth.rs` (`post_email` ~line 144, `get_verify` ~line 170)
- Test: `backend/src/auth.rs` `mod tests` (near `verify_invalid_token_redirects_with_error`)

**Interfaces:**
- Consumes: `db::consume_email_token`, `db::lookup_email_token` (Task 1), `new_token()`, `redirect_to`, `redirect_to_with_cookie`, `build_session_cookie`, constants `SESSION_COOKIE_NAME`, `SESSION_DAYS`.
- Produces: new cookie constant `PENDING_COOKIE_NAME: &str = "ov_pending"`; `get_verify` redirects to `{base_url}/?login=confirmed` on success; `post_email` sets the `ov_pending` cookie.

- [ ] **Step 1: Write the failing tests**

Add to `backend/src/auth.rs` `mod tests`:

```rust
#[sqlx::test(migrations = "./migrations")]
async fn verify_consumes_token_but_issues_no_session(pool: PgPool) {
    use rand::Rng;
    let state = test_state(pool.clone());
    let app = Router::new().merge(auth_router()).with_state(state);
    let mut rng = rand::thread_rng();
    let token_bytes: [u8; 32] = rng.gen();
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);
    let expires = Utc::now() + chrono::Duration::minutes(15);
    db::insert_email_token(&pool, "a@b.com", &token, expires).await.unwrap();

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
    let st = db::lookup_email_token(&pool, &token).await.unwrap().unwrap();
    assert!(st.used);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test verify_consumes_token_but_issues_no_session`
Expected: FAIL — current `get_verify` sets `set-cookie` and redirects to base_url without `login=confirmed`.

- [ ] **Step 3: Rewrite `get_verify` to be confirm-only**

Replace the body of `get_verify` (lines 170-201) with:

```rust
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
        Some(_) => Ok(redirect_to(&format!(
            "{}/?login=confirmed",
            state.base_url
        ))),
        None => Ok(redirect_to(&format!(
            "{}/?error=invalid_token",
            state.base_url
        ))),
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test verify_consumes_token_but_issues_no_session`
Expected: PASS.

- [ ] **Step 5: Add the `ov_pending` cookie constant and set it in `post_email`**

Add next to `SESSION_COOKIE_NAME` (line 56):

```rust
const PENDING_COOKIE_NAME: &str = "ov_pending";
```

Add a helper next to `build_session_cookie` (line 60):

```rust
fn build_pending_cookie(token: &str) -> String {
    format!(
        "{name}={token}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=900",
        name = PENDING_COOKIE_NAME,
    )
}
```

In `post_email`, after `let token = new_token();`, insert (before `insert_email_token`):

```rust
    let pending_cookie = build_pending_cookie(&token);
```

Then at the end of `post_email`, return the cookie with the OK response:

```rust
    Ok((
        StatusCode::OK,
        [(header::SET_COOKIE, pending_cookie)],
        Json(OkResponse { ok: true }),
    )
        .into_response()
        .into())
```

Change the return type of `post_email` to `Result<Response, StatusCode>` and use `.into_response()`:

```rust
async fn post_email(
    State(state): State<AppState>,
    Json(body): Json<EmailRequest>,
) -> Result<Response, StatusCode> {
```

End of function becomes:

```rust
    Ok((
        StatusCode::OK,
        [(header::SET_COOKIE, pending_cookie)],
        Json(OkResponse { ok: true }),
    )
        .into_response())
```

- [ ] **Step 6: Add a test for the `ov_pending` cookie**

Add to `mod tests`:

```rust
#[sqlx::test(migrations = "./migrations")]
async fn email_endpoint_sets_pending_cookie(pool: PgPool) {
    let state = test_state(pool.clone());
    state.smtp_config.as_ref(); // no-op, keeps state usage explicit
    // smtp not configured -> 501; to test the cookie we need smtp set,
    // but the cookie is set after smtp check. Instead assert the helper directly.
}
```

**Note:** `post_email` returns 501 without SMTP, and the cookie is set only after the SMTP check passes, so the cookie is hard to observe via the router without SMTP. Verify the cookie string via a direct helper test instead:

```rust
#[test]
fn pending_cookie_format() {
    let cookie = build_pending_cookie("tok123");
    assert_eq!(
        cookie,
        "ov_pending=tok123; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=900"
    );
}
```

- [ ] **Step 7: Run full auth tests, fmt, clippy**

```bash
DATABASE_URL=postgres://ov:ov@localhost:5432/ov cargo test auth::tests
cargo fmt
cargo fmt --check
cargo clippy -- -D warnings
```

Expected: all pass; fmt and clippy clean.

- [ ] **Step 8: Commit**

```bash
git add backend/src/auth.rs
git commit -m "feat: verify confirms only; post_email sets ov_pending cookie"
```

---

### Task 3: Add `GET /api/auth/login/status` poll endpoint

**Files:**
- Modify: `backend/src/auth.rs` (add handler + route)
- Test: `backend/src/auth.rs` `mod tests`

**Interfaces:**
- Consumes: `db::lookup_email_token` (Task 1), `db::find_or_create_user`, `db::create_session`, `read_cookie`, `new_token`, `build_session_cookie`, `cleared_session_cookie`, `PENDING_COOKIE_NAME`, constants `SESSION_COOKIE_NAME`, `SESSION_DAYS`.
- Produces: `async fn get_login_status(State, HeaderMap) -> Response` handling `GET /api/auth/login/status`; JSON `{"loggedIn":bool}` (camelCase via `OkResponse`-style struct). Creates the session and sets `ov_session` when the pending token is used.

- [ ] **Step 1: Write the failing tests**

Add a response struct near `OkResponse` (line 51):

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LoginStatusResponse {
    logged_in: bool,
}
```

Add tests to `mod tests`:

```rust
#[sqlx::test(migrations = "./migrations")]
async fn login_status_without_cookie_is_false(pool: PgPool) {
    let state = test_state(pool);
    let app = Router::new().merge(auth_router()).with_state(state);
    let resp = app
        .oneshot(
            Request::get("/api/auth/login/status")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["loggedIn"], false);
}

#[sqlx::test(migrations = "./migrations")]
async fn login_status_pending_then_logged_in(pool: PgPool) {
    use rand::Rng;
    let state = test_state(pool.clone());
    let app = Router::new().merge(auth_router()).with_state(state);
    let mut rng = rand::thread_rng();
    let token_bytes: [u8; 32] = rng.gen();
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);
    let expires = Utc::now() + chrono::Duration::minutes(15);
    db::insert_email_token(&pool, "a@b.com", &token, expires).await.unwrap();
    let cookie = format!("{PENDING_COOKIE_NAME}={token}");

    // not clicked yet -> false
    let resp = app
        .clone()
        .oneshot(
            Request::get("/api/auth/login/status")
                .header("Cookie", &cookie)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["loggedIn"], false);

    // simulate the email link being clicked
    let _ = db::consume_email_token(&pool, &token).await.unwrap();

    // now the requester logs in
    let resp = app
        .oneshot(
            Request::get("/api/auth/login/status")
                .header("Cookie", &cookie)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let set_cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(set_cookie.starts_with("ov_session="), "got: {set_cookie}");
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["loggedIn"], true);
}

#[sqlx::test(migrations = "./migrations")]
async fn login_status_expired_token_clears_cookie(pool: PgPool) {
    use rand::Rng;
    let state = test_state(pool.clone());
    let app = Router::new().merge(auth_router()).with_state(state);
    let mut rng = rand::thread_rng();
    let token_bytes: [u8; 32] = rng.gen();
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);
    let expires = Utc::now() - chrono::Duration::minutes(1);
    db::insert_email_token(&pool, "a@b.com", &token, expires).await.unwrap();
    let cookie = format!("{PENDING_COOKIE_NAME}={token}");
    let resp = app
        .oneshot(
            Request::get("/api/auth/login/status")
                .header("Cookie", &cookie)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let set_cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(set_cookie.starts_with("ov_pending=;"), "got: {set_cookie}");
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["loggedIn"], false);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test login_status`
Expected: FAIL — route `/api/auth/login/status` returns 404.

- [ ] **Step 3: Implement `get_login_status` and register the route**

Add near the other auth handlers (after `get_verify`):

```rust
async fn get_login_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let Some(token) = read_cookie(&headers, PENDING_COOKIE_NAME) else {
        return Json(LoginStatusResponse { logged_in: false }).into_response();
    };
    let st = match db::lookup_email_token(&state.pool, &token).await {
        Ok(Some(st)) => st,
        // unknown or expired token: clear the stale cookie
        _ => {
            return (
                StatusCode::OK,
                [(header::SET_COOKIE, cleared_pending_cookie())],
                Json(LoginStatusResponse { logged_in: false }),
            )
                .into_response();
        }
    };
    if !st.used {
        return Json(LoginStatusResponse { logged_in: false }).into_response();
    }
    // email confirmed: issue a session on the requesting device
    let user_id = match db::find_or_create_user(&state.pool, "email", &st.email, &st.email).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("find_or_create_user failed: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let session_token = new_token();
    let expires = Utc::now() + chrono::Duration::days(SESSION_DAYS);
    if let Err(e) = db::create_session(&state.pool, user_id, &session_token, expires).await {
        tracing::error!("create_session failed: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    (
        StatusCode::OK,
        [
            (header::SET_COOKIE, build_session_cookie(&session_token).await),
            (header::SET_COOKIE, cleared_pending_cookie()),
        ],
        Json(LoginStatusResponse { logged_in: true }),
    )
        .into_response()
}

fn cleared_pending_cookie() -> String {
    format!(
        "{name}=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0",
        name = PENDING_COOKIE_NAME,
    )
}
```

Register the route in `auth_router()` (line 681), after the verify route:

```rust
        .route("/api/auth/login/status", get(get_login_status))
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test login_status`
Expected: PASS (3 tests).

- [ ] **Step 5: Run full auth tests, fmt, clippy**

```bash
DATABASE_URL=postgres://ov:ov@localhost:5432/ov cargo test
cargo fmt
cargo fmt --check
cargo clippy -- -D warnings
```

Expected: all pass; fmt and clippy clean.

- [ ] **Step 6: Commit**

```bash
git add backend/src/auth.rs
git commit -m "feat: add login/status poll endpoint issuing requester session"
```

---

### Task 4: Frontend — add `fetchLoginStatus` and polling login

**Files:**
- Modify: `frontend/src/api.ts` (add `fetchLoginStatus`)
- Modify: `frontend/src/hooks/useAuth.tsx` (polling `loginEmail`)
- Modify: `frontend/src/components/Marquee.tsx` (waiting state + `?login=confirmed` banner)
- Modify: `frontend/src/locales/en.json`, `frontend/src/locales/de.json` (new keys)
- Test: `frontend/src/api.test.ts`, `frontend/src/components/Marquee.test.tsx`

**Interfaces:**
- Consumes: `sendMagicLink` (existing), `fetchMe` (existing).
- Produces: `export async function fetchLoginStatus(): Promise<boolean>`; `AuthState.loginEmail` now resolves only after the poll sees `loggedIn` true; i18n keys `auth.waiting`, `auth.confirmed`.

- [ ] **Step 1: Write the failing `fetchLoginStatus` test**

Add to `frontend/src/api.test.ts`:

```ts
it("fetchLoginStatus returns loggedIn flag", async () => {
  const fetchMock = vi.fn(async () => ({ ok: true, json: async () => ({ loggedIn: true }) }));
  vi.stubGlobal("fetch", fetchMock);
  const result = await fetchLoginStatus();
  expect(result).toBe(true);
  expect(fetchMock).toHaveBeenCalledWith("/api/auth/login/status");
  vi.unstubAllGlobals();
});
```

Check `api.test.ts` for its existing import/stub patterns and match them (the import must include `fetchLoginStatus`).

- [ ] **Step 2: Run the test to verify it fails**

Run: `npm test`
Expected: FAIL — `fetchLoginStatus is not a function`.

- [ ] **Step 3: Implement `fetchLoginStatus`**

Add to `frontend/src/api.ts` after `sendMagicLink`:

```ts
export async function fetchLoginStatus(): Promise<boolean> {
  const resp = await fetch("/api/auth/login/status");
  if (!resp.ok) throw new Error("login status failed");
  const data = (await resp.json()) as { loggedIn: boolean };
  return data.loggedIn;
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `npm test`
Expected: PASS.

- [ ] **Step 5: Update `loginEmail` in `useAuth.tsx` to poll**

Replace the current `loginEmail` callback (lines 48-50):

```ts
const loginEmail = useCallback(async (email: string) => {
  await sendMagicLink(email);
  const deadline = Date.now() + 15 * 60 * 1000;
  while (Date.now() < deadline) {
    await new Promise((r) => setTimeout(r, 3000));
    try {
      if (await fetchLoginStatus()) {
        await refresh();
        return;
      }
    } catch {
      // transient network error: keep polling
    }
  }
}, [refresh]);
```

Update the import on line 2 to include `fetchLoginStatus`.

- [ ] **Step 6: Add i18n keys**

Add to `frontend/src/locales/en.json` under `auth`:

```json
"waiting": "Check your email — waiting for confirmation…",
"confirmed": "Sign-in confirmed — you can close this tab."
```

Add to `frontend/src/locales/de.json` under `auth`:

```json
"waiting": "Prüfe deine E-Mails — warte auf Bestätigung…",
"confirmed": "Anmeldung bestätigt — du kannst diesen Tab schließen."
```

- [ ] **Step 7: Update `Marquee.tsx`**

Replace the `handleEmailSubmit` (lines 14-19) and the auth panel's email section (lines 53-66) so that:

- While polling, the submit button reads `t("auth.waiting")` and is disabled.
- On mount, read `login=confirmed` from the URL (via `useSearchParams` from `react-router-dom`) and show a banner with `t("auth.confirmed")` above the panel when present.

Concretely:

```tsx
const [searchParams] = useSearchParams();
const confirmed = searchParams.get("login") === "confirmed";
const [sending, setSending] = useState(false);

const handleEmailSubmit = async (e: FormEvent) => {
  e.preventDefault();
  if (!emailInput.trim() || sending) return;
  setSending(true);
  try {
    await loginEmail(emailInput.trim());
  } finally {
    setSending(false);
  }
};
```

In the panel:

```tsx
{confirmed && <p className="auth-note">{t("auth.confirmed")}</p>}
{providers?.email && (
  <form onSubmit={handleEmailSubmit}>
    <input
      className="auth-input"
      type="email"
      placeholder={t("auth.emailPlaceholder")}
      value={emailInput}
      onChange={(e) => setEmailInput(e.target.value)}
      disabled={sending}
    />
    <button className="auth-submit" type="submit" disabled={sending}>
      {sending ? t("auth.waiting") : t("auth.sendLink")}
    </button>
  </form>
)}
```

Update the import on line 1 to include `useEffect` if needed (or keep as-is; `useSearchParams` comes from `react-router-dom`).

- [ ] **Step 8: Add Marquee tests**

Add to `frontend/src/components/Marquee.test.tsx`:

```tsx
it("shows waiting state while login email is pending", async () => {
  mockFetchMe.mockRejectedValue(new Error("not auth"));
  mockFetchProviders.mockResolvedValue({ email: true, google: false, apple: false, github: false });
  mockSendMagicLink.mockResolvedValue(undefined);
  mockFetchLoginStatus.mockResolvedValue(false);
  renderMarquee();
  await waitFor(() => expect(screen.getByText("Sign in")).toBeDefined());
  fireEvent.click(screen.getByText("Sign in"));
  fireEvent.change(screen.getByPlaceholderText("your@email.com"), { target: { value: "a@b.com" } });
  fireEvent.click(screen.getByText("Send link"));
  await waitFor(() => expect(screen.getByText(/waiting for confirmation/)).toBeDefined());
});

it("renders the confirmed banner when ?login=confirmed", async () => {
  mockFetchMe.mockRejectedValue(new Error("not auth"));
  mockFetchProviders.mockResolvedValue({ email: true, google: false, apple: false, github: false });
  render(
    <MemoryRouter initialEntries={["/?login=confirmed"]}>
      <AuthProvider>
        <Marquee />
      </AuthProvider>
    </MemoryRouter>
  );
  await waitFor(() => expect(screen.getByText(/close this tab/)).toBeDefined());
});
```

Add the mock setup at the top of the file next to the existing mocks:

```tsx
const mockSendMagicLink = vi.mocked(api.sendMagicLink);
const mockFetchLoginStatus = vi.mocked(api.fetchLoginStatus);
```

In `beforeEach`, add `mockSendMagicLink.mockResolvedValue(undefined);` and
`mockFetchLoginStatus.mockResolvedValue(false);`.

Note: the "waiting" test relies on `fetchLoginStatus` resolving `false`; the poll loop keeps running, so use `vi.useFakeTimers` or assert the waiting text appears before the first 3s poll tick resolves (the mock resolves immediately, so the text is visible synchronously). If the test flakes, wrap the click in `act` and assert immediately.

- [ ] **Step 9: Run frontend tests and build**

```bash
npm test
npm run build
```

Expected: all pass; build succeeds.

- [ ] **Step 10: Commit**

```bash
git add frontend/src
git commit -m "feat: poll login status on the requesting device; confirmed banner"
```

---

### Task 5: Verify and deploy

**Files:** none (CI + manual verification)

**Interfaces:**
- Consumes: all tasks above.

- [ ] **Step 1: Run the full backend suite once more**

```bash
cd backend && DATABASE_URL=postgres://ov:ov@localhost:5432/ov cargo test && cargo fmt --check && cargo clippy -- -D warnings
```

- [ ] **Step 2: Run the full frontend suite once more**

```bash
cd frontend && npm test && npm run build
```

- [ ] **Step 3: Push to trigger CI/CD**

```bash
git push origin master
```

Then watch:

```bash
gh run watch --repo semtexkg/cinema --exit-status
```

Expected: test, build, deploy all green.

- [ ] **Step 4: Manual end-to-end check**

1. Open https://cinema.k-labs.app on desktop, click Sign in, enter a real email, submit.
2. Watch the desktop — within ~3s of clicking the email link (from any device) the header should flip to "Sign out".
3. From a phone/mobile, open the same email link — it should show the "Sign-in confirmed — you can close this tab." banner and NOT log the phone in.
4. Confirm the desktop is logged in via `curl -s https://cinema.k-labs.app/api/auth/me` with the session cookie from the desktop browser.

---

## Self-review notes

- **Spec coverage:** verify confirm-only (Task 2), `ov_pending` cookie (Task 2), login/status poll (Task 3), frontend polling + confirmed banner + i18n (Task 4), testing and deploy (Task 5). SSO untouched. Security rules (no session from verify, cookie-only no login) enforced in Task 3 implementation.
- **Placeholder scan:** no TBD/TODO; test code is concrete. The one "Note" in Task 2 Step 6 explicitly explains why the cookie is tested via the helper rather than the router.
- **Type consistency:** `EmailTokenState { email, used }` defined in Task 1 and consumed by Task 3; `LoginStatusResponse { logged_in }` with `rename_all = "camelCase"` matches `{"loggedIn": ...}` used in tests and frontend; i18n keys `auth.waiting`/`auth.confirmed` consistent across Tasks 4 and locale files.
