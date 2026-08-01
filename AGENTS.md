# OV-Kino Linz

Watcher that detects new OV/OmU showings at Cineplexx Linz and Megaplex PlusCity,
sends Telegram alerts to the public channel `@ov_linz`, and serves a small web page
of upcoming showings. Python/Flask, no DB — state lives in JSON files under `DATA_DIR`.

## Layout

- `app/fetchers.py` — cinema program fetchers (cineplexx, megaplex); each returns `(showings, movie_metas)`
- `app/checker.py` — dedup/pruning + check orchestration (`Config`, `run_check`); caches poster images under `DATA_DIR/posters/`
- `app/notify.py` — Telegram alerts (`send_telegram`)
- `app/web.py` — read-only web UI (`create_app(data_dir)`); serves cached posters at `/posters/<name>`
- `app/ics.py` — ICS calendar feed renderer (`render_ics`), served at `/showings.ics`
- `app/state.py` — `save_showings`/`load_showings` JSON persistence; `showings.json` carries a `"movies"` map (`"Cinema|Title"` → runtime/genres/poster) alongside the flat `"showings"` list.
- `app/main.py` — entrypoint: scheduler loop + web server (env: `DATA_DIR`, `PORT`,
  `CHECK_INTERVAL_HOURS`, `TELEGRAM_BOT_TOKEN`, `TELEGRAM_CHAT_ID`, `SOURCES`)

## Running the web UI locally

Use `./serve.sh` — starts `app/web.py` only (no scheduler, no network fetches):

```
./serve.sh start     # serve http://localhost:8080, seeds demo data if ./data is empty
./serve.sh stop      # via PID file ./data/web.pid
./serve.sh restart
./serve.sh status
```

Overrides: `PORT=9090 ./serve.sh start`, `DATA_DIR=/tmp/x ./serve.sh start`.
Delete `./data/showings.json` and restart to re-seed demo data.

## Tests

```
python3 -m venv .venv && .venv/bin/pip install -r requirements.txt -r requirements-dev.txt
.venv/bin/python -m pytest -q
```

## Background jobs via the bash tool

Long-running/backgrounded processes launched through the bash tool hold the tool's
output pipe open, so the tool waits out its timeout even though the process started
fine. Launch detached instead:

```
( setsid cmd </dev/null >/tmp/x.log 2>&1 & )
```

Prefer `serve.sh` over ad-hoc server launches.
