# Remove Apple SSO — Design Spec

**Date:** 2026-08-08
**Status:** Draft

## Overview

Remove Apple as a login provider entirely. It was implemented but never
enabled in production (no credentials were ever configured; prod reports
`"apple":false`), and it won't be coming. Removing it cleans up dead
configuration, a dead crate dependency, CI/Helm wiring, GitHub secrets, and
the frontend button.

## Scope

Full removal — backend, frontend, infra, docs, and secrets.

## Backend

- `backend/src/auth.rs`:
  - Drop `AppleConfig` from the `use crate::web::{...}` import.
  - Drop `ProvidersResponse.apple` field and its `state.apple_oauth.is_some()` assignment.
  - Drop the `"apple"` arm from `oidc_issuer`.
  - Drop the entire `apple_client_secret` function (JWT signing for Apple's client secret).
  - Drop the `"apple"` arm from `oidc_client`.
  - Drop `sso_apple` and `sso_apple_callback` handlers.
  - Drop the two routes `/api/auth/sso/apple` and `/api/auth/sso/apple/callback`.
  - Drop the two test references (`apple_oauth: None` state, `json["apple"]` assertion).
  - Keep `oidc_issuer`, `oidc_client`, `sso_callback_oidc` (Google still uses them).
- `backend/src/web.rs`: drop the `AppleConfig` struct, the `apple_oauth` field on `AppState`, and the four `apple_oauth: None` test-state entries.
- `backend/src/main.rs`: drop the `apple_oauth` match block.
- `backend/src/config.rs`: drop `apple_client_id`, `apple_team_id`, `apple_key_id`, `apple_private_key` fields, their env reads, and their test assertions.
- `backend/src/checker.rs`: drop the four `apple_*: None` fields in the test state.
- `backend/Cargo.toml`: drop `jsonwebtoken = "9"` — it is used only by `apple_client_secret` (verified: no other references).

## Frontend

- `frontend/src/types.ts`: drop `apple` from `AuthProviders`.
- `frontend/src/components/LoginModal.tsx`: drop the Apple SSO button branch.
- `frontend/src/index.css`: drop `.auth-sso` (dead once Apple is gone; Google/GitHub move to `.auth-provider` in the next plan).
- Tests: strip `apple: false` from `LoginModal.test.tsx`, `Marquee.test.tsx`, `LoginConfirmedPage.test.tsx`.

## Infra + docs + secrets

- `helm/ov-watcher/values.yaml`: drop `appleClientId`, `appleTeamId`, `appleKeyId`, `applePrivateKey`.
- `helm/ov-watcher/templates/secret.yaml`: drop the four `APPLE_*` entries.
- `.github/workflows/deploy.yml`: drop the `APPLE_PRIVATE_KEY` env block and the four `--set secrets.apple*` lines.
- `AGENTS.md`: drop the four Apple secrets from the cluster-facts list.
- Delete GitHub Actions secrets `APPLE_CLIENT_ID`, `APPLE_TEAM_ID`, `APPLE_KEY_ID`, `APPLE_PRIVATE_KEY`.

## API shape change

Removing the `apple` field from `ProvidersResponse` changes
`GET /api/auth/providers` from `{"email":true,"google":true,"apple":false,"github":true}` to `{"email":true,"google":true,"github":true}`. The frontend type updates in lockstep, so this is safe.

## Out of scope

- The login-modal polish (frosted glass + official SSO buttons) — separate plan.
- Removing `user_identities` rows with `provider = 'apple'` (none exist; Apple was never enabled).
