# Dev Stack via Docker Compose — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run the whole dev stack (Postgres + Rust backend + React frontend) in containers with hot reload via a new `docker-compose.dev.yml`.

**Architecture:** Three compose services — `db` (reused from the existing compose), `backend` (rust image, `cargo watch -x run`, source bind-mounted, cargo-target named volume), `frontend` (node image, `npm run dev`, source bind-mounted, anonymous node_modules volume). The Vite proxy target becomes configurable so the container can point at `http://backend:8080` while native dev keeps `http://localhost:8080`.

**Tech Stack:** Docker Compose 2.x, Docker 29.x, Rust (cargo-watch), Node 22 + Vite, Postgres 17.

## Global Constraints

- New file `docker-compose.dev.yml`; do NOT modify the existing `docker-compose.yml` or the production `Dockerfile`.
- Host ports match native dev: `db`=5432, `backend`=8080, `frontend`=5173.
- Backend service: image `rust:1-slim-bookworm`, workdir `/build`, command `cargo watch -x run`, bind `./backend:/build`, named volume `cargo-target:/build/target`, bind `./data:/data`, env `DATABASE_URL=postgres://ov:ov@db:5432/ov`, `PORT=8080`, `DATA_DIR=/data`, optional `TELEGRAM_BOT_TOKEN`/`TELEGRAM_CHAT_ID` passthrough, `depends_on: db: condition: service_healthy`, host port 8080.
- Frontend service: image `node:22-alpine`, workdir `/app`, command `npm run dev`, bind `./frontend:/app`, anonymous volume `/app/node_modules`, env `VITE_PROXY_TARGET=http://backend:8080`, host port 5173.
- `frontend/vite.config.ts` proxy target reads `process.env.VITE_PROXY_TARGET` with default `http://localhost:8080`.
- `cargo-watch` installed inside the backend dev image at build time (`cargo install cargo-watch --locked`), not in the entrypoint.
- `.dockerignore` already covers `**/target`, `**/node_modules`, `frontend/dist`, `data`, `.git`, `docs` — no change needed.
- Docker on this box needs `sudo` until the group change (Task 3); verification commands in Tasks 1-2 use `sudo -n docker compose` (passwordless sudo is available).

---

### Task 1: Backend dev image and `docker-compose.dev.yml`

**Files:**
- Create: `backend/Dockerfile.dev`
- Create: `docker-compose.dev.yml`

**Interfaces:**
- Consumes: existing `.dockerignore`, existing `docker-compose.yml` db service as reference.
- Produces: `docker-compose.dev.yml` with services `db`, `backend`, `frontend`; `backend/Dockerfile.dev` image used by the backend service.

- [ ] **Step 1: Create `backend/Dockerfile.dev`**

```dockerfile
FROM rust:1-slim-bookworm
RUN cargo install cargo-watch --locked
WORKDIR /build
```

- [ ] **Step 2: Create `docker-compose.dev.yml`**

```yaml
services:
  db:
    image: postgres:17-alpine
    environment:
      POSTGRES_USER: ov
      POSTGRES_PASSWORD: ov
      POSTGRES_DB: ov
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U ov"]
      interval: 2s
      timeout: 3s
      retries: 20

  backend:
    build:
      context: .
      dockerfile: backend/Dockerfile.dev
    working_dir: /build
    command: cargo watch -x run
    volumes:
      - ./backend:/build
      - cargo-target:/build/target
      - ./data:/data
    environment:
      DATABASE_URL: postgres://ov:ov@db:5432/ov
      PORT: "8080"
      DATA_DIR: /data
      TELEGRAM_BOT_TOKEN: ${TELEGRAM_BOT_TOKEN:-}
      TELEGRAM_CHAT_ID: ${TELEGRAM_CHAT_ID:-}
    ports:
      - "8080:8080"
    depends_on:
      db:
        condition: service_healthy

  frontend:
    image: node:22-alpine
    working_dir: /app
    command: npm run dev
    volumes:
      - ./frontend:/app
      - /app/node_modules
    environment:
      VITE_PROXY_TARGET: http://backend:8080
    ports:
      - "5173:5173"
    depends_on:
      - backend

volumes:
  pgdata: {}
  cargo-target: {}
```

- [ ] **Step 3: Validate the compose file**

Run: `sudo -n docker compose -f docker-compose.dev.yml config`
Expected: the file parses; services `db`, `backend`, `frontend` and volumes `pgdata`, `cargo-target` listed with the values above.

- [ ] **Step 4: Build the backend dev image (validates the Dockerfile)**

Run: `sudo -n docker compose -f docker-compose.dev.yml build backend`
Expected: builds successfully (installs cargo-watch). Note: this takes a few minutes the first time (rust base image + cargo install).

- [ ] **Step 5: Commit**

```bash
git add backend/Dockerfile.dev docker-compose.dev.yml
git commit -m "feat: add dev stack compose file and backend dev image"
```

---

### Task 2: Configurable Vite proxy target

**Files:**
- Modify: `frontend/vite.config.ts`

**Interfaces:**
- Consumes: existing proxy block (lines 8-13) hard-coding `http://localhost:8080`.
- Produces: proxy target read from `process.env.VITE_PROXY_TARGET`, defaulting to `http://localhost:8080`; used by all four paths (`/api`, `/posters`, `/showings.ics`, `/healthz`).

- [ ] **Step 1: Modify `vite.config.ts`**

Replace the `server.proxy` block so the target comes from an env var with the localhost default:

```ts
const proxyTarget = process.env.VITE_PROXY_TARGET || "http://localhost:8080";

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      "/api": proxyTarget,
      "/posters": proxyTarget,
      "/showings.ics": proxyTarget,
      "/healthz": proxyTarget,
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: "./src/test/setup.ts",
  },
});
```

- [ ] **Step 2: Run the frontend tests to confirm nothing broke**

Run: `npm test` from `frontend/`
Expected: all tests pass (the proxy config is dev-server-only; tests use jsdom and don't hit it).

- [ ] **Step 3: Commit**

```bash
git add frontend/vite.config.ts
git commit -m "feat: make vite proxy target configurable"
```

---

### Task 3: Docker group membership, LOCAL_DEV.md, and bring-up verification

**Files:**
- Modify: `LOCAL_DEV.md`
- (System change, not a repo file: `sudo usermod -aG docker semtex`)

**Interfaces:**
- Consumes: Tasks 1-2.
- Produces: `docker compose -f docker-compose.dev.yml up` documented as the dev workflow; `semtex` in the `docker` group.

- [ ] **Step 1: Add semtex to the docker group (system change)**

Run: `sudo usermod -aG docker semtex`
Then verify group membership: `id semtex` → `docker` appears in the group list.
Note: the new group only applies to new login sessions; for the current session use `newgrp docker` or log out/in.

- [ ] **Step 2: Update `LOCAL_DEV.md`**

Replace the "1. Start Postgres" / "2. Run the backend" / "3. Run the frontend dev server" sections' primary workflow with the compose dev stack. Concretely, add a prominent section near the top (after "Prerequisites") and mark the manual steps as the alternative. New section:

```markdown
## Dev stack (recommended)

Everything in one command — Postgres, backend (auto-recompiles on change),
and the Vite dev server:

```bash
docker compose -f docker-compose.dev.yml up
```

- Backend on http://localhost:8080, frontend on http://localhost:5173.
- Backend recompiles via `cargo watch` on save; the frontend hot-reloads.
- The backend's cargo `target/` lives in a named volume, so rebuilds stay
  incremental across restarts.
- `TELEGRAM_BOT_TOKEN` / `TELEGRAM_CHAT_ID` are passed through from a `.env`
  file in the repo root if present (see the optional Telegram section below).

Stop with `Ctrl-C`; bring everything down with `docker compose -f
docker-compose.dev.yml down`.
```

Keep the existing manual steps (docker compose up -d db + cargo run + npm run
dev) as the "Manual alternative" below it. Note in the frontend dev-server
subsection that `VITE_PROXY_TARGET` overrides the proxy target (default
`http://localhost:8080`).

- [ ] **Step 3: Bring up the stack and verify end-to-end**

Stop the natively-running processes first (the backend from earlier on 8080 and the Vite server on 5173 would conflict with the container ports):

```bash
# kill the background cargo run and the vite dev server if running
pkill -f 'target/debug/ov-watcher' || true
pkill -f 'vite --host' || true
```

Then:

```bash
sudo -n docker compose -f docker-compose.dev.yml up -d
sleep 5
curl -s localhost:8080/healthz          # expect: ok
curl -s localhost:5173/api/auth/providers   # expect: JSON (email/google/etc:false)
curl -s localhost:5173/api/showings     # expect: JSON with cinemas (proxied through Vite to the backend)
```

Expected: all three checks return data. The frontend proxy must work through
`VITE_PROXY_TARGET=http://backend:8080` (proven by the 5173 API responses).

- [ ] **Step 4: Commit the docs change**

```bash
git add LOCAL_DEV.md
git commit -m "docs: document the docker compose dev stack"
```

---

### Task 4: Final verification

**Files:** none

- [ ] **Step 1: Bring the stack down and confirm clean state**

```bash
sudo -n docker compose -f docker-compose.dev.yml down
```

Expected: containers removed; volumes remain (pgdata, cargo-target) so the
next `up` is fast.

- [ ] **Step 2: Restart the stack once more to confirm reproducibility**

```bash
sudo -n docker compose -f docker-compose.dev.yml up -d
sleep 5
curl -s localhost:8080/healthz   # expect ok
```

Expected: backend comes back without a full rebuild (cargo-target volume
cache), healthz ok. Leave the stack running.

---

## Self-review notes

- **Spec coverage:** all three services (Task 1), configurable Vite proxy (Task 2), docker-group + LOCAL_DEV.md + bring-up (Task 3), reproducibility check (Task 4). `.dockerignore` verified to already cover the dev mounts — no change, matching spec.
- **Placeholder scan:** all file contents are concrete; no TBD/TODO.
- **Type consistency:** service names (`db`, `backend`, `frontend`), volume names (`pgdata`, `cargo-target`), env var (`VITE_PROXY_TARGET`), and host ports match the spec exactly. The compose `frontend` service uses `depends_on: backend` (loose) and `db` uses `condition: service_healthy`, as specified.
