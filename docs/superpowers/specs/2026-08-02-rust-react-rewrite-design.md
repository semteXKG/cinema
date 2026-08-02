# Rewrite: Rust Backend + React SPA + Postgres — Design

- Date: 2026-08-02
- Status: Approved by user
- Supersedes the implementation stack of `2026-07-18-cinema-ov-watcher-design.md`
  (behavior and data sources stay as documented there)

## Motivation

The Python/Flask app works well, but the user wants to move to a more common
tech stack as the app grows. Considered options: TypeScript/NestJS, Spring Boot
(Java/Kotlin), .NET, Rust — each with a React SPA and Postgres.

Decision: **Rust backend + React SPA + Postgres**, because:

- The user already works professionally with Spring Boot — rewriting in Java
  teaches nothing new.
- The React/TypeScript frontend covers the "professionally relevant,
  widely-hired" part of the goal.
- The Rust backend is the deliberate learning part of the project. The app is
  small, self-contained and non-critical, which makes it a good Rust
  playground.

Explicitly accepted trade-off: Rust's performance/safety advantages are not
needed for this I/O-bound app; development will be slower. The point is
learning, not necessity.

## Goal

Feature parity with the current app, on the new stack:

1. Periodic checks for new English OV/OmU showings at Cineplexx Linz and
   Megaplex PlusCity (unchanged sources and OV-detection logic).
2. Telegram alerts to `@ov_linz` (same German message format, same chunking).
3. Web page listing all upcoming OV showings (same visual design, now a React
   SPA talking to a JSON API).
4. ICS calendar feed at `/showings.ics` (same output format).
5. Single-container deployment in the user's k8s cluster (plus Postgres).

The one deliberate architectural change: **JSON state files are replaced by
Postgres**, which also seeds future features (history, stats).

## Non-goals (YAGNI)

- No new user-facing features beyond parity
- No auth, no multi-user, no admin UI
- No history/stats UI yet (`check_run` only enables it later)
- No data migration beyond the one-shot dedup seed (see Cutover)

## Stack

### Backend (Rust)

| Concern | Choice | Notes |
|---|---|---|
| Web framework | axum | serves API + static React build (via `tower-http::ServeDir`) |
| Async runtime | tokio | scheduler loop via `tokio::time::interval` (matches `CHECK_INTERVAL_HOURS` semantics) |
| HTTP client | reqwest | cinema sources, Telegram API, poster downloads |
| HTML parsing | scraper | CSS selectors — BeautifulSoup equivalent (Megaplex) |
| JSON | serde / serde_json | Cineplexx API + own API responses |
| Database | sqlx (Postgres), runtime-checked queries | no ORM; `sqlx-cli` migrations in `backend/migrations/` |
| Errors | thiserror (modules) + anyhow (top level) | |
| Logging | tracing + tracing-subscriber | |
| ICS | hand-rolled | 1:1 port of `ics.py`; no crate |
| OV-label regexes | regex crate | 1:1 port of `models.py` patterns |

### Frontend

React 18 + TypeScript + Vite. Plain CSS (the existing stylesheet, ported).
Data via `fetch` with a 15-minute re-poll (mirrors today's `<meta refresh=900>`).
Vite dev server proxies `/api`, `/posters`, `/showings.ics`, `/healthz` to the
backend.

### Infrastructure

- Local dev: `docker-compose.yml` with Postgres; backend run via `cargo run`,
  frontend via `vite dev`.
- Prod: one multi-stage Dockerfile (node build → cargo build → debian-slim
  runtime, non-root), same single-pod deployment; Postgres added to the
  cluster manifests (or external).

## Architecture

One process, like today: the Rust binary runs the axum server and a background
tokio task for the periodic check. The React app is built to static files and
served by the same binary.

```
cinema/
├── backend/
│   ├── Cargo.toml
│   ├── migrations/            # sqlx migrations
│   ├── src/
│   │   ├── main.rs            # config from env, spawn scheduler, serve axum
│   │   ├── models.rs          # Showing/Movie + OV-label matching (port of models.py)
│   │   ├── fetchers/
│   │   │   ├── mod.rs         # SourceFetcher trait, SourceError
│   │   │   ├── cineplexx.rs   # pure parse fns over &str + thin HTTP layer
│   │   │   └── megaplex.rs    # same split
│   │   ├── checker.rs         # one check run: fetch -> diff -> notify -> persist (port of checker.py)
│   │   ├── notify.rs          # Telegram formatting + sending (port of notify.py)
│   │   ├── ics.rs             # ICS rendering (port of ics.py)
│   │   ├── db.rs              # sqlx queries (movies, showings, dedup, pruning)
│   │   └── web.rs             # routes: /api/showings, /showings.ics, /posters/<n>, /healthz, SPA statics
│   └── tests/                 # unit + integration tests, reusing tests/fixtures/*
├── frontend/                  # Vite + React + TS
├── tests/fixtures/            # existing recorded Cineplexx/Megaplex fixtures (reused by Rust tests)
├── docker-compose.yml
├── Dockerfile                 # multi-stage
├── k8s/ + helm/               # updated manifests (+ Postgres)
└── docs/
```

Module boundaries mirror the current Python modules 1:1, making the port
mechanical and keeping units small and independently testable. Fetchers keep
the pure-parse/thin-HTTP split so parser tests need no network.

## Data model (Postgres)

```sql
CREATE TABLE movie (
  id          BIGSERIAL PRIMARY KEY,
  cinema      TEXT NOT NULL,
  title       TEXT NOT NULL,
  runtime_min INT,
  genres      TEXT[] NOT NULL DEFAULT '{}',
  poster_url  TEXT,
  poster_file TEXT,
  UNIQUE (cinema, title)
);

CREATE TABLE showing (
  id            BIGSERIAL PRIMARY KEY,
  movie_id      BIGINT NOT NULL REFERENCES movie(id) ON DELETE CASCADE,
  start         TIMESTAMPTZ NOT NULL,
  version       TEXT NOT NULL,
  hall          TEXT NOT NULL DEFAULT '',
  url           TEXT NOT NULL DEFAULT '',
  first_seen_at TIMESTAMPTZ NOT NULL,
  UNIQUE (movie_id, start)
);

CREATE TABLE source_status (
  source               TEXT PRIMARY KEY,
  status               TEXT NOT NULL,          -- 'ok' | 'error'
  last_error_ping_date DATE
);

CREATE TABLE check_run (
  id          BIGSERIAL PRIMARY KEY,
  run_at      TIMESTAMPTZ NOT NULL,
  new_count   INT NOT NULL,
  total_count INT NOT NULL
);
```

- Dedup replaces `state.json`'s `seen` set: insert with
  `ON CONFLICT (movie_id, start) DO NOTHING`; rows actually inserted are the
  "new" showings for the alert. `first_seen_at` keeps today's metadata.
- Pruning per run: `DELETE FROM showing WHERE start < now() - INTERVAL '6 hours'`
  (same 6h grace as today), then delete orphan movies (no remaining showings).
- `source_status` replaces `error_pings`; `check_run` replaces
  `showings.json`'s `generated_at` (latest run = the footer's "last checked").
- Poster cache stays on disk under `DATA_DIR/posters/`; `movie.poster_file`
  holds the basename. Same filename scheme (sha1 of URL) and same pruning of
  unused files.

## API contract

`GET /api/showings` — exactly the view model the current template consumes:

```json
{
  "generatedAt": "2026-08-02T12:00:00+02:00",
  "sources": { "cineplexx": "ok", "megaplex": "error" },
  "cinemas": [
    {
      "name": "Megaplex PlusCity",
      "movies": [
        {
          "title": "The Odyssey",
          "badge": "OmU",
          "metaLine": "Drama · 121 Min",
          "poster": "a1b2c3d4e5f6.jpg",
          "showings": [
            { "date": "Mo 04.08.", "time": "19:30", "detail": "Saal 3", "url": "https://…" }
          ]
        }
      ]
    }
  ]
}
```

- `cinemas: null` → "first check running"; `[]` → "no showings" (three states,
  as today).
- Cinema ordering (Megaplex first, then alphabetical), badge/meta-line/detail
  derivation, and date formatting all replicate `web.py`'s `_group_showings`.
- `GET /showings.ics` (text/calendar), `GET /posters/{name}` (1-day cache
  header), `GET /healthz` — behavior unchanged.
- All other GET paths fall back to `index.html` (SPA).

## Check-run behavior (parity)

Same semantics as `checker.py`:

1. Fetch enabled sources (`SOURCES` env, default `cineplexx,megaplex`).
2. Per-source failure → status `error` + Telegram error ping, rate-limited to
   one per source per day (via `source_status.last_error_ping_date`).
3. Keep only showings with `start >= now`.
4. Insert movies (`ON CONFLICT (cinema, title)` update metadata) and showings
   (`ON CONFLICT DO NOTHING`); inserted rows = new showings.
5. Download missing posters best-effort (retry next run), prune unused files.
6. If new showings and Telegram is configured: one message, same German HTML
   format, chunked at 4096 chars on line boundaries.
7. Prune old showings/movies; insert a `check_run` row.

The run's DB mutations execute in one transaction; the Telegram send happens
after commit (no network I/O held inside a transaction). Poster files are the
other exception — filesystem, reconciled at the end of the run.

## Config (env, unchanged names where possible)

`TELEGRAM_BOT_TOKEN`, `TELEGRAM_CHAT_ID`, `CHECK_INTERVAL_HOURS` (default 3),
`SOURCES` (default `cineplexx,megaplex`), `DATA_DIR` (posters; default
`./data`), `PORT` (default 8080), `DATABASE_URL`.

## Cutover

The new DB starts empty, so the first run would re-notify all current showings
as "new". Mitigation: a small one-shot importer (subcommand,
`ov-watcher import-state <data_dir>`) that reads the existing
`showings.json`/`state.json` and seeds `movie`/`showing` rows (with
`first_seen_at` from the old `seen` timestamps) before the first real run.
Alternative accepted during discussion: skip the importer and tolerate one
duplicate Telegram message. Default: implement the importer; it's small.

Old Python app stays in git history; the rewrite replaces it on a feature
branch and the old `app/`/`tests/` Python files are removed in the same PR
(fixtures are kept and reused).

## Deployment

- `Dockerfile` (multi-stage):
  1. `node:22-alpine` — `npm ci && npm run build` (frontend)
  2. `rust:1-slim` — `cargo build --release` (backend; SQLX_OFFLINE not needed
     since queries are runtime-checked)
  3. `debian:bookworm-slim` + ca-certificates, non-root user — binary +
     `frontend/dist` + `DATA_DIR` volume
- `docker-compose.yml`: Postgres 17 + the app (for local dev/tests).
- k8s: update `deployment.yaml` (image, env, probes against `/healthz`), and
  add an in-cluster Postgres (StatefulSet + Service + PVC) as the default;
  Secret gains `DATABASE_URL` (can point at an external instance instead by
  just changing the value, in which case the Postgres manifests are skipped).
  Helm chart updated likewise.

## Error handling

- Parity with today (see Check-run behavior): per-source isolation, 1/day
  error ping, best-effort posters, corrupt-input resilience handled at the
  parse layer (`scraper`/`serde` errors → `SourceError`).
- Rust-specific: no `unwrap`/`panic` in request paths; fetcher/notify errors
  are typed (`thiserror`) and converted to health statuses, never crashes.

## Testing

- TDD throughout, mirroring the current suite:
  - Parser unit tests reusing `tests/fixtures/*` (Cineplexx JSON, Megaplex
    HTML) — parse functions are pure (`&str` → structs).
  - OV-label matcher tests (port of `test_models.py`).
  - Checker tests with fake fetchers + mock Telegram sender (trait objects),
    covering dedup, pruning, error-ping rate limiting (port of
    `test_checker.py`).
  - ICS golden tests (port of `test_ics.py`).
  - Telegram formatting/chunking tests (port of `test_notify.py`).
  - API tests against a test Postgres (sqlx test harness or
    testcontainers-style docker Postgres) for `db.rs` + `/api/showings`.
- Frontend: Vitest + Testing Library for grouping/rendering components.
- CI (GitHub Actions): `cargo test`, `cargo clippy -- -D warnings`,
  `cargo fmt --check`, frontend `vitest run` + `tsc --noEmit`.

## Risks / caveats

- **Rust learning curve** — accepted; the module-by-module port keeps problems
  small and comparable against the Python reference implementation and its
  test suite.
- **sqlx compile-time checks** — skipped deliberately (runtime-checked
  queries) to keep the build self-contained; the query count is small.
- **scraper vs. BeautifulSoup behavior differences** — mitigated by reusing
  the recorded fixtures; parity assertions come from the ported tests.
- **Single binary serving SPA** — axum `ServeDir` + fallback covers this; no
  separate web server needed.
