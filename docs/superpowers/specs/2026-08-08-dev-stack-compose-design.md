# Dev Stack via Docker Compose — Design Spec

**Date:** 2026-08-08
**Status:** Draft

## Overview

Run the whole dev stack (Postgres + Rust backend + React frontend) in
containers with hot reload, replacing the native-process setup currently
documented in LOCAL_DEV.md. A new `docker-compose.dev.yml` brings up all three
services with source bind-mounted so edits are picked up immediately: the
backend recompiles via `cargo watch`, the frontend hot-reloads via Vite.

## Behavior

`docker compose -f docker-compose.dev.yml up` starts:

- `db` — Postgres 17 on host port 5432 (same as today).
- `backend` — the Rust app on host port 8080, recompiling on source change.
- `frontend` — the Vite dev server on host port 5173, hot-reloading on source
  change, proxying `/api`, `/posters`, `/showings.ics`, `/healthz` to the
  backend service.

Ports and behavior match the native setup, so existing workflows (curl
localhost:8080, open localhost:5173) are unchanged.

## Services

### db

Reused from `docker-compose.yml` (postgres:17-alpine, user/pass/db `ov`,
host port 5432, named volume `pgdata`, healthcheck gating the other services).

### backend

- Image: `rust:1-slim-bookworm` (matches the prod Dockerfile's build stage).
- Working dir: `/build`.
- Command: `cargo watch -x run` (cargo-watch installed via
  `cargo install cargo-watch --locked` at build time).
- Bind mount: `./backend:/build` (source).
- Named volume `cargo-target:/build/target` so incremental builds survive
  container restarts (avoids full rebuilds each `up`).
- Bind mount `./data:/data` for poster caching.
- Env: `DATABASE_URL=postgres://ov:ov@db:5432/ov`, `PORT=8080`,
  `DATA_DIR=/data`, plus optional `TELEGRAM_BOT_TOKEN` / `TELEGRAM_CHAT_ID` /
  SMTP vars passed through from the host `.env` (empty when unset, matching
  current config behavior).
- `depends_on: db: condition: service_healthy`.
- Exposes host port 8080.

### frontend

- Image: `node:22-alpine`.
- Working dir: `/app`.
- Command: `npm run dev`.
- Bind mount: `./frontend:/app` (source).
- Anonymous volume over `/app/node_modules` so the container's `npm ci`
  install is not shadowed by the host mount (and vice versa).
- Vite proxy target configurable: env `VITE_PROXY_TARGET`, default
  `http://localhost:8080` (native mode); compose sets it to
  `http://backend:8080`.
- Exposes host port 5173.
- `depends_on: backend` (loose — Vite proxies lazily).

## Frontend change: configurable Vite proxy

`frontend/vite.config.ts` currently hard-codes `http://localhost:8080` as the
proxy target. Change it to read `process.env.VITE_PROXY_TARGET` with that
same default, so native dev keeps working and the container passes the
service hostname.

## Setup steps

- Run `cargo install cargo-watch --locked` once inside the backend build (in
  the image, not the container entrypoint) so `up` doesn't wait on it.
- Add `semtex` to the `docker` group:
  `sudo usermod -aG docker semtex` — one-time system change so `docker` /
  `docker compose` work without `sudo`. Effective after re-login
  (`newgrp docker` for the current session).
- Document the new workflow in LOCAL_DEV.md: `docker compose -f
  docker-compose.dev.yml up` replaces the manual `docker compose up -d db` +
  `cargo run` + `npm run dev` steps.

## Files

- Create: `docker-compose.dev.yml`
- Create: `backend/Dockerfile.dev` (backend dev image: rust base + cargo-watch)
- Modify: `frontend/vite.config.ts` (configurable proxy target)
- Modify: `LOCAL_DEV.md` (dev-stack workflow)
- Modify: `.dockerignore` (already excludes `target`, `node_modules`,
  `frontend/dist`, `data` — verify it covers the dev mounts)

## Out of scope

- VS Code devcontainer / Codespaces (user chose compose, self-hosted).
- Changing the production `Dockerfile` or `docker-compose.yml`.
- Containerized test running — tests still run natively per LOCAL_DEV.md.
