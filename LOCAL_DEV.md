# Local Development

How to run the OV Cinema watcher locally (Rust backend + React frontend + Postgres).

## Prerequisites

- Rust toolchain (`cargo` / `rustc`) — e.g. via rustup
- Node.js 22 + npm
- Docker Engine + Compose (`docker compose` — see note below)

## Dev stack (recommended)

Everything in one command — Postgres, backend (auto-recompiles on change),
and the Vite dev server:

```bash
docker compose -f docker-compose.dev.yml up
```

> First time on a fresh clone, install the frontend dependencies once first:
> `cd frontend && npm install` (node_modules is gitignored and the dev stack
> reuses the host copy).

> The first `up` pulls the Rust image, installs `cargo-watch`, and does a full
> compile before http://localhost:8080 responds — be patient.

- Backend on http://localhost:8080, frontend on http://localhost:5173.
- Backend recompiles via `cargo watch` on save; the frontend hot-reloads.
- The backend's cargo `target/` lives in a named volume, so rebuilds stay
  incremental across restarts.
- `TELEGRAM_BOT_TOKEN` / `TELEGRAM_CHAT_ID` are passed through from a `.env`
  file in the repo root if present (see the optional Telegram section below).

Stop with `Ctrl-C`; bring everything down with `docker compose -f
docker-compose.dev.yml down`.

## Manual alternative

Run each piece natively instead — Postgres via the root compose file, backend
via `cargo`, frontend via Vite.

### 1. Start Postgres

```bash
docker compose up -d db        # Postgres 17 on localhost:5432, user ov / password ov / db ov
```

> Docker user group: after installing Docker, log out and back in (or `newgrp docker`)
> so `docker` works without `sudo`. Until then use `sudo docker ...`.
>
> On this machine a native Postgres was previously installed and used; it has been
> **stopped and disabled** so it doesn't grab port 5432. It can be re-enabled with
> `sudo systemctl enable --now postgresql` if ever needed.

Every shell that runs the app or tests needs:

```bash
export DATABASE_URL=postgres://ov:ov@localhost:5432/ov
```

### 2. Run the backend

```bash
cd backend
cargo run            # http://localhost:8080
```

On startup the binary:

1. runs the database migrations,
2. starts the web server (API + ICS + posters + `healthz`),
3. fires the first check run immediately (fetches both cinemas — needs internet),
   then repeats every `CHECK_INTERVAL_HOURS` (default 3).

Check it's alive: `curl localhost:8080/healthz` → `ok`.

Endpoints:

| URL | Purpose |
|---|---|
| `/healthz` | liveness probe |
| `/api/showings` | JSON view model the SPA consumes |
| `/showings.ics` | calendar feed |
| `/posters/<file>` | cached poster images |
| `/` | the built React SPA (if `STATIC_DIR` resolves to a built `frontend/dist`) |

### Serving the SPA from the backend (optional)

`STATIC_DIR` defaults to `./frontend/dist` **relative to the current working directory**.
If you run `cargo run` from inside `backend/`, that path doesn't exist and `/` returns 404.
Either point it at the real build:

```bash
cd backend
STATIC_DIR=../frontend/dist cargo run        # after: cd frontend && npm run build
```

…or just use the Vite dev server (recommended, next section) instead.

### 3. Run the frontend dev server (for UI work)

```bash
cd frontend
npm install
npm run dev          # http://localhost:5173
```

Vite proxies `/api`, `/posters`, `/showings.ics`, `/healthz` to `http://localhost:8080`,
so keep the backend running on 8080. You get hot reload; the backend serves data.
`VITE_PROXY_TARGET` overrides the proxy target (default `http://localhost:8080`)
— the compose stack sets it to `http://backend:8080`.

## 4. Tests

```bash
cd backend && cargo test          # needs DATABASE_URL set (Postgres running)
cd frontend && npm test
```

## 5. Local dev cheat sheet

```bash
docker compose up -d db                                  # 1. Postgres (or native, see above)
export DATABASE_URL=postgres://ov:ov@localhost:5432/ov   # 2. every shell
cd backend && cargo run                                  # 3a. API on :8080
cd frontend && npm install && npm run dev                # 3b. (optional) SPA on :5173
```

## Common issues

- **Port 8080 already in use** — a stale process (e.g. the old Python app, or a leftover
  `ov-watcher`) is holding it. Find and kill it: `ss -tlnp | grep 8080`, then `kill <pid>`.
- **`/` returns 404 from the backend** — `STATIC_DIR` doesn't point at a built
  `frontend/dist`. Build the frontend and set `STATIC_DIR`, or use the Vite dev server.
- **Sources show `error` in the UI** — the check run couldn't reach a cinema site
  (network / site changed). It retries on the next run; the app keeps working.
- **A Telegram message appears on first run with a fresh DB** — the DB has no
  "seen" state yet, so every current showing is new once. This is expected and only
  happens once (later runs deduplicate via `ON CONFLICT`).
- **`cargo test` fails to connect** — Postgres isn't running or `DATABASE_URL` isn't
  exported in that shell.

## Optional: Telegram notifications locally

The app runs fine without them (it still fetches and stores showings). To enable alerts:

```bash
export TELEGRAM_BOT_TOKEN=...
export TELEGRAM_CHAT_ID=@ov_linz    # or any chat/channel id
```

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

## Cluster Postgres (production)

```bash
./dev/connectPostgres.sh                 # interactive psql in the cluster
./dev/connectPostgres.sh "SELECT ..."    # one-shot query
```

SSHes to the cluster node (default `semtex@10.0.0.5`, override via `NODE=...`),
pulls the password from the `ov-watcher-secret` k8s secret, execs into
`ov-watcher-postgres-0`. Tables: `movie`, `showing`, `source_status`, `check_run`.
