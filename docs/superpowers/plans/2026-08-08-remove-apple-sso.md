# Remove Apple SSO — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove Apple as a login provider across backend, frontend, infra, docs, and GitHub secrets.

**Architecture:** Mechanical removal. Backend: drop AppleConfig/fields/routes/`apple_client_secret`/`jsonwebtoken`. Frontend: drop the `apple` type field and button. Infra: drop Helm/CI wiring and delete the secrets. `oidc_issuer`/`oidc_client`/`sso_callback_oidc` stay (Google uses them).

**Tech Stack:** Rust (axum, sqlx), React 19 + Vite + TypeScript, Helm, GitHub Actions, `gh` CLI.

## Global Constraints

- `GET /api/auth/providers` result becomes `{"email":true,"google":true,"github":true}` (no `apple` field) — frontend type updated in lockstep.
- Do NOT remove `oidc_issuer`, `oidc_client`, `sso_callback_oidc`, or the `openidconnect` crate (Google uses them).
- `jsonwebtoken = "9"` in `backend/Cargo.toml` is removed ONLY if no other usage exists (verified: only `apple_client_secret`).
- Backend tests need `DATABASE_URL=postgres://ov:ov@localhost:5432/ov`.
- Frontend tests run from `frontend/` with `npm test`; build with `npm run build`.

---

### Task 1: Backend removal

**Files:**
- Modify: `backend/src/auth.rs`, `backend/src/web.rs`, `backend/src/main.rs`, `backend/src/config.rs`, `backend/src/checker.rs`, `backend/Cargo.toml`

**Interfaces:**
- Produces: `AppState` without `apple_oauth`; `ProvidersResponse` without `apple`; `auth_router()` without Apple routes; no `apple_client_secret`; no `jsonwebtoken` dep.

- [ ] **Step 1: Remove Apple from `backend/src/auth.rs`**

Remove:
- `AppleConfig` from the `use crate::web::{AppState, AppleConfig, OAuthConfig};` import → `use crate::web::{AppState, OAuthConfig};`
- `apple: bool,` from `ProvidersResponse` (line ~42) and `apple: state.apple_oauth.is_some(),` (line ~301)
- the `"apple" => Ok(...)` arm in `oidc_issuer` (line ~322)
- the whole `fn apple_client_secret` (lines ~327-348)
- the `"apple" => { ... }` arm in `oidc_client` (lines ~359-364)
- `async fn sso_apple` (lines ~442-443) and `async fn sso_apple_callback` (lines ~601-606)
- the routes `.route("/api/auth/sso/apple", get(sso_apple))` and `.route("/api/auth/sso/apple/callback", get(sso_apple_callback))` (lines ~778-779)
- `apple_oauth: None,` in the test state (line ~804) and `assert_eq!(json["apple"], false);` (line ~826)

- [ ] **Step 2: Remove Apple from `backend/src/web.rs`**

Remove the `AppleConfig` struct (lines ~25-29), `pub apple_oauth: Option<AppleConfig>,` from `AppState` (line ~39), and the four `apple_oauth: None,` test-state entries (lines ~435, 524, 563, 625).

- [ ] **Step 3: Remove Apple from `backend/src/main.rs`**

Remove the `apple_oauth: match (...)` block (lines ~131-144).

- [ ] **Step 4: Remove Apple from `backend/src/config.rs`**

Remove `apple_client_id`, `apple_team_id`, `apple_key_id`, `apple_private_key` fields (lines ~22-25), their `get("APPLE_*")` reads (lines ~84-87), and test assertions (lines ~163-166 and ~220).

- [ ] **Step 5: Remove Apple from `backend/src/checker.rs`**

Remove the four `apple_*: None` fields in the test state (lines ~346-349).

- [ ] **Step 6: Drop `jsonwebtoken` from `backend/Cargo.toml`**

Remove the `jsonwebtoken = "9"` line. Then confirm no remaining usage: `rg -n "jsonwebtoken" backend/src/` returns nothing.

- [ ] **Step 7: Run backend tests, fmt, clippy**

```bash
DATABASE_URL=postgres://ov:ov@localhost:5432/ov cargo test
cargo fmt
cargo fmt --check
cargo clippy -- -D warnings
```

Expected: all pass; fmt and clippy clean.

- [ ] **Step 8: Commit**

```bash
git add backend
git commit -m "feat: remove Apple SSO from backend"
```

---

### Task 2: Frontend removal

**Files:**
- Modify: `frontend/src/types.ts`, `frontend/src/components/LoginModal.tsx`, `frontend/src/index.css`, `frontend/src/components/LoginModal.test.tsx`, `frontend/src/components/Marquee.test.tsx`, `frontend/src/pages/LoginConfirmedPage.test.tsx`

**Interfaces:**
- Consumes: `AppState` without `apple_oauth` (Task 1).
- Produces: `AuthProviders` without `apple`; LoginModal without the Apple button; `.auth-sso` CSS removed.

- [ ] **Step 1: Remove `apple` from `frontend/src/types.ts`**

Remove `apple: boolean;` from the `AuthProviders` interface (line ~34).

- [ ] **Step 2: Remove the Apple button from `frontend/src/components/LoginModal.tsx`**

Remove the `{providers?.apple && (...)}` block (lines ~70-74).

- [ ] **Step 3: Remove `.auth-sso` from `frontend/src/index.css`**

Remove the `.auth-sso` and `.modal .auth-sso` rules (lines ~166-168, ~182). Confirm no other usage: `rg -n "auth-sso" frontend/src/` returns nothing after the modal change.

- [ ] **Step 4: Strip `apple: false` from the test files**

Remove `apple: false,` from `LoginModal.test.tsx` (line ~37), `Marquee.test.tsx` (lines ~44, 58, 78, 92, 111), and `LoginConfirmedPage.test.tsx` (line ~30).

- [ ] **Step 5: Run frontend tests and build**

```bash
cd frontend && npm test && npm run build
```

Expected: all pass; build succeeds.

- [ ] **Step 6: Commit**

```bash
git add frontend
git commit -m "feat: remove apple provider from frontend"
```

---

### Task 3: Infra, docs, and secrets

**Files:**
- Modify: `helm/ov-watcher/values.yaml`, `helm/ov-watcher/templates/secret.yaml`, `.github/workflows/deploy.yml`, `AGENTS.md`

**Interfaces:**
- Consumes: Tasks 1-2.
- Produces: no `APPLE_*` references anywhere in the repo; the four GitHub secrets deleted.

- [ ] **Step 1: Remove Apple from `helm/ov-watcher/values.yaml`**

Remove `appleClientId`, `appleTeamId`, `appleKeyId`, `applePrivateKey` (lines ~27-30).

- [ ] **Step 2: Remove Apple from `helm/ov-watcher/templates/secret.yaml`**

Remove the four `APPLE_*` entries (lines ~11-14).

- [ ] **Step 3: Remove Apple from `.github/workflows/deploy.yml`**

Remove the `APPLE_PRIVATE_KEY` env block (lines ~134-136) and the four `--set secrets.apple*` lines (lines ~143-146). Also remove the `env:` block's `APPLE_PRIVATE_KEY` if it's only used there.

- [ ] **Step 4: Remove Apple from `AGENTS.md`**

Remove `APPLE_CLIENT_ID`, `APPLE_TEAM_ID`, `APPLE_KEY_ID`, `APPLE_PRIVATE_KEY` from the cluster-facts list (lines ~68-69).

- [ ] **Step 5: Confirm no remaining references**

```bash
rg -in "apple" --glob '!docs/**' --glob '*.rs' --glob '*.ts' --glob '*.tsx' --glob '*.yml' --glob '*.yaml' --glob '*.md' --glob '!frontend/src/index.css' .
```

Expected: only incidental matches (e.g. `-apple-system` in css, fixture content). No `apple_oauth`, `APPLE_CLIENT`, `sso_apple`, `appleClient`.

- [ ] **Step 6: Delete the GitHub secrets**

```bash
gh secret delete APPLE_CLIENT_ID --repo semtexkg/cinema
gh secret delete APPLE_TEAM_ID --repo semtexkg/cinema
gh secret delete APPLE_KEY_ID --repo semtexkg/cinema
gh secret delete APPLE_PRIVATE_KEY --repo semtexkg/cinema
```

Verify: `gh secret list --repo semtexkg/cinema | grep APPLE` returns nothing.

- [ ] **Step 7: Commit**

```bash
git add helm .github AGENTS.md
git commit -m "chore: drop Apple SSO from helm, CI, and secrets"
```

---

### Task 4: Verify and deploy

**Files:** none

**Interfaces:**
- Consumes: Tasks 1-3.

- [ ] **Step 1: Run both suites once more**

```bash
cd backend && DATABASE_URL=postgres://ov:ov@localhost:5432/ov cargo test && cargo fmt --check && cargo clippy -- -D warnings
cd frontend && npm test && npm run build
```

- [ ] **Step 2: Push and watch CI**

```bash
git push origin master
gh run watch --repo semtexkg/cinema --exit-status
```

Expected: test, build, deploy all green.

- [ ] **Step 3: Verify the API shape**

```bash
curl -s https://cinema.k-labs.app/api/auth/providers
```

Expected: `{"email":true,"google":true,"github":true}` (no `apple` field).

---

## Self-review notes

- **Spec coverage:** backend (Task 1), frontend (Task 2), infra/docs/secrets (Task 3), verify+deploy (Task 4). OIDC helpers kept for Google; `jsonwebtoken` dropped only after verifying no other usage.
- **Placeholder scan:** all concrete; the one grep in Task 3 Step 5 expects only incidental matches, stated explicitly.
- **Type consistency:** `AuthProviders` drops `apple` in lockstep with `ProvidersResponse`; the expected `providers` JSON shape matches the frontend type.
