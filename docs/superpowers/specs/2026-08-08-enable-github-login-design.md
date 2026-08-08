# Enable GitHub Login — Design Spec

**Date:** 2026-08-08
**Status:** Draft

## Overview

GitHub SSO is already fully implemented: backend OAuth flow
(`sso_github` / `sso_github_callback`, `backend/src/auth.rs:446, 609`), the
frontend "Sign in with GitHub" button (gated on `providers.github`), and the
Helm/CI secret wiring (`GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`). Prod
currently reports `"github":false` because those two secrets are unset. This
spec covers creating the GitHub OAuth app, setting the secrets, deploying, and
verifying.

Open to **any GitHub user** (no org restriction).

## GitHub OAuth app

Created by the human in their GitHub account settings
(https://github.com/settings/developers → New OAuth App):

| Field | Value |
|-------|-------|
| Application name | `OV Cinema Linz` |
| Homepage URL | `https://cinema.k-labs.app` |
| Authorization callback URL | `https://cinema.k-labs.app/api/auth/sso/github/callback` |

The app yields a **Client ID** and a **Client Secret**.

## Backend behavior (already implemented, no changes)

- `GET /api/auth/sso/github` → 302 to `https://github.com/login/oauth/authorize` with `client_id`, `redirect_uri` (above), `scope=user:email`, `state` (CSRF, cookie-bound, 10-min TTL).
- `GET /api/auth/sso/github/callback` → validates state cookie, exchanges code, fetches `/user` (numeric id) and `/user/emails` (primary verified email), then `find_or_create_user("github", id, email)` and issues a session.
- Returns `501` if `GITHUB_CLIENT_ID`/`GITHUB_CLIENT_SECRET` unset (which is why the button is hidden today).

## Secrets

Set as GitHub Actions repository secrets (consumed by
`.github/workflows/deploy.yml:147-148`):

| Secret | Value |
|--------|-------|
| `GITHUB_CLIENT_ID` | the OAuth app's Client ID |
| `GITHUB_CLIENT_SECRET` | the OAuth app's Client Secret |

## Deployment

Next push to `master` triggers the existing pipeline; the deploy job passes
`--set secrets.githubClientId=...` and `--set secrets.githubClientSecret=...`
into the Helm Secret. The pod picks up the new env vars. `GET
/api/auth/providers` flips `"github": true`, and the login panel shows the
GitHub button.

Note: unlike the SMTP secret, no manual `kubectl patch` is needed — a push
that includes any repo change runs the pipeline. If no repo change is
pending, push a trivial commit (or use the running dev stack locally with the
secrets exported to verify the flow before prod).

## Verification

1. `curl https://cinema.k-labs.app/api/auth/providers` → `"github": true`.
2. In the browser, open the login panel → "Sign in with GitHub" button visible.
3. Click it → GitHub authorize screen → allow → redirected back to `/`, logged in.
4. `curl -s /api/auth/me` with the session cookie returns the GitHub email.
5. The same GitHub account used via both Google and GitHub links to one user
   (identity model: login matches `(provider, provider_id)`, then email).

## Out of scope

- Org-restricted GitHub login (decision: open to all).
- New backend/frontend code — none needed.
- Verifying against the local dev stack with the secrets — optional, via
  `export GITHUB_CLIENT_ID=... GITHUB_CLIENT_SECRET=...` on the dev backend.
