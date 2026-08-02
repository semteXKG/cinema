# OV-Kino Linz

Watcher that detects new OV/OmU showings at Cineplexx Linz and Megaplex PlusCity,
sends Telegram alerts to the public channel `@ov_linz`, and serves a web page
of upcoming showings. Rust backend (axum + Postgres) with a React frontend.
State lives in Postgres; poster images are cached under `DATA_DIR/posters/`.

## Layout

- `backend/` — Rust/axum: `models.rs`, `fetchers/` (cineplexx, megaplex),
  `checker.rs` (dedup/pruning + check orchestration), `notify.rs` (Telegram),
  `ics.rs` (calendar feed), `db.rs` (Postgres), `web.rs` (API + static files),
  `main.rs` (entrypoint: scheduler loop + web server).
- `frontend/` — React + Vite; dev server proxies `/api`, `/healthz`, `/showings.ics`.
- `k8s/`, `helm/`, `docker-compose.yml`, `Dockerfile` — deployment.
- State in Postgres; posters cached under `DATA_DIR/posters/`.

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

## Tests

```
cd backend && cargo test          # needs DATABASE_URL (docker compose up -d db);
                                  # #[sqlx::test] creates per-test DBs
cd frontend && npm test
```
