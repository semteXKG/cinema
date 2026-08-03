# OV-Kino Linz

Watcher that detects new OV/OmU showings at Cineplexx Linz and Megaplex PlusCity,
sends Telegram alerts to `@ov_linz`, and serves upcoming showings on a web page.
Rust backend (axum + sqlx/Postgres), React frontend, deployed via GitHub Actions
+ Helm. Production: https://cinema.k-labs.app

## Layout

- `backend/` — Rust/axum: `models.rs`, `fetchers/` (cineplexx, megaplex),
  `checker.rs` (dedup/pruning + check orchestration), `notify.rs` (Telegram),
  `ics.rs` (calendar feed), `db.rs` (sqlx; migrations in `backend/migrations/`),
  `web.rs` (API + static files), `main.rs` (scheduler loop + web server).
- `frontend/` — React 19 + Vite + TypeScript; dev server proxies
  `/api`, `/healthz`, `/showings.ics`, `/posters`.
- `helm/ov-watcher/` — production chart: Deployment + Postgres StatefulSet +
  Traefik IngressRoute + cert-manager TLS (`values.yaml` documents all knobs).
- `dev/connectPostgres.sh` — psql into the cluster Postgres (SSH to the node,
  password pulled from the k8s secret).
- `Dockerfile` — multi-stage: cargo-chef (dependency cache layer) →
  Rust build → npm build → runtime image.

State in Postgres; posters cached on the app PVC under `DATA_DIR/posters/`.

Env vars (`backend/src/config.rs`): `DATABASE_URL` (required), `DATA_DIR`,
`STATIC_DIR`, `PORT`, `CHECK_INTERVAL_HOURS`, `TELEGRAM_BOT_TOKEN`,
`TELEGRAM_CHAT_ID`, `SOURCES`.

## Running locally

```
docker compose up -d db
export DATABASE_URL=postgres://ov:ov@localhost:5432/ov
cd backend && cargo run          # http://localhost:8080
cd frontend && npm install && npm run dev   # optional Vite dev server
```

Details: `LOCAL_DEV.md`.

## Tests

```
cd backend && cargo test          # needs DATABASE_URL (docker compose up -d db);
                                  # #[sqlx::test] creates per-test DBs
cd frontend && npm test
```

## CI/CD (push to master)

`.github/workflows/deploy.yml`, three jobs:

1. **test** (`ubuntu-latest`) — fmt, clippy, cargo test (service Postgres), npm
   test + build.
2. **build** (`ubuntu-24.04-arm`, native arm64 — repo is public, so the free
   arm64 runners are available; QEMU builds took >1h, native ~3min) — builds and
   pushes `ghcr.io/semtexkg/cinema:<sha>` + `:latest`. Dependency caching via
   cargo-chef stage + `type=gha` buildx cache.
3. **deploy** (self-hosted `arc-runner-cinema`) — `helm upgrade --install` into
   namespace `default` with `--set image.tag=<sha>`,
   `--set secrets.telegramBotToken=...`, `--set postgres.password=...`, then
   `kubectl rollout status`.

Cluster facts:

- GitHub secrets: `TELEGRAM_BOT_TOKEN`, `POSTGRES_PASSWORD`
  (must stay stable — it seeds the Postgres StatefulSet and the app's
  `DATABASE_URL`; changing it breaks existing pods).
- The deploy runner's service account needs cluster access — granted via
  ClusterRoleBinding `arc-runner-admin` (`arc-runners:arc-runner-sa` →
  cluster-admin). Recreate if the binding is lost:
  `kubectl create clusterrolebinding arc-runner-admin --clusterrole=cluster-admin --serviceaccount=arc-runners:arc-runner-sa`
- Prod resources (ns `default`): Deployment `ov-watcher`, StatefulSet
  `ov-watcher-postgres`, Service `ov-watcher`, IngressRoute + cert
  `cinema.k-labs.app` (Let's Encrypt via cert-manager ClusterIssuer
  `letsencrypt-prod`).
- Cluster Postgres inspect: `./dev/connectPostgres.sh [SQL]` (SSH to
  `semtex@10.0.0.5`, exec into `ov-watcher-postgres-0`).
