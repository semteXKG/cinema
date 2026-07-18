# Cinema OV Watcher — Design

- Date: 2026-07-18
- Status: Approved by user

## Problem

The user misses English original-version showings (OV / OmU) at the two Linz
cinemas — **Cineplexx Linz** and **Hollywood Megaplex PlusCity** — or notices
them too late, when good seats are gone. OV showings are rare, scattered among
many German-dubbed showings, and appear on the cinema websites up to ~14 days
ahead.

## Goal

An automated watcher that:

1. Detects **new** English OV/OmU showings at both cinemas and alerts via
   **Telegram** (with a direct booking link per showing), so the user can book
   seats immediately.
2. Serves a **web page** listing *all* currently known upcoming OV/OmU
   showings at both cinemas, for anytime lookup.
3. Runs in the user's **local k8s cluster** as a single pod.

## Non-goals (YAGNI)

- Coming-soon movies without scheduled showings
- Seat-availability tracking
- Alerts for removed/rescheduled showings
- Auth, multi-user, external exposure of the web page (internal cluster only)
- Aggregator data sources (film.at): the user explicitly chose direct cinema
  sources only

## Data sources (verified against live data on 2026-07-18)

### Cineplexx Linz — JSON API

- Base URL: `https://app.cineplexx.at`
- Required headers:
  - `CINEPLEXX-Platform: WEB`
  - `client-key: 308330b1-52a5-4883-aee3-304240c22ea1`
- Cinema ID: `1014`
- Step 1: `GET /api/v1/cinemasweb/1014/movies?date=all`
  → list of movie master objects (fields used: `id`, `title`, `shortURL`)
- Step 2: `GET /api/v2/moviesweb/{movieId}/sessions?location=AUT`
  → `[{"sessions": [...]}]` covering ~14 days, all cinemas;
  filter `cinemaId == "1014"`.
- Session fields used: `showtime` (ISO 8601), `screenName`, `technologies`,
  `conceptAttributesNames`.
- **OV detection:** any string in `technologies[0]` matching
  `\b(OV|OmU|OmdU)\b` (e.g. `"OV (Englisch)"`), or `conceptAttributesNames`
  containing `OV`/`OmU`. If a parenthesized language is present, it must be
  `Englisch`; bare tags are accepted (OV/OmU at Cineplexx Linz is virtually
  always English).
- Booking link: `https://cineplexx.at/film/{shortURL}` (film page listing the
  showtimes; verified live, e.g. `/film/die-odyssee`).
- Far-out sessions are already bookable (sales channels include WWW).

### Hollywood Megaplex PlusCity — server-rendered HTML

- Step 1: `GET https://www.megaplex.at/kinoprogramm/linz/{YYYY-MM-DD}/ov`
  for each of the next 14 days → OV-filtered program page; extract film links
  of the form `/film/linz/{slug}/ov`.
- Step 2: `GET https://www.megaplex.at/film/linz/{slug}/ov`
  → showtime boxes grouped under day headers (`Heute`, `Morgen`,
  `Montag, 20.07.2026`, ...). Each box carries a version label
  (e.g. `OV - IMAX 2D`), a time, and a `/ticket/{...}` href.
- **OV detection:** version label starts with `OV`. Megaplex tags all
  original-language showings as `OV` (no separate OmU label exists on the
  site).
- Booking link: `https://www.megaplex.at` + the `/ticket/...` href (direct
  booking link, verified in markup).
- Requests use a desktop browser User-Agent and a small (~0.5 s) delay
  between page fetches.

## Architecture

One container image, one pod, three units:

1. **Checker** — fetchers (Cineplexx, Megaplex) → normalize → diff against
   `state.json` → Telegram alert on new showings. On every run it also writes
   `showings.json` (all currently known upcoming OV/OmU showings) for the web
   page. Showings whose start time lies in the past are pruned from both
   `showings.json` and the `state.json` dedup keys.
2. **Web** — Flask app:
   - `/` renders `showings.json`: showings grouped by date, per cinema, with
     version badges, hall, booking links, a "last checked" timestamp, and
     per-source health (ok/error). Server-rendered, meta auto-refresh,
     read-only, no auth.
   - `/healthz` for k8s liveness/readiness probes.
3. **Scheduler** — background thread in the same pod, runs the checker every
   `CHECK_INTERVAL_HOURS` (default 3). Chosen over a k8s CronJob so checker
   and web share state in-pod with a simple RWO volume.

### Data model

Normalized showing (produced by both fetchers):

```
(cinema, movie_title, start_time_iso, version_label, hall, booking_url)
```

Dedup key: `cinema|movie_title|start_time_iso`

State files on a PVC mounted at `/data`:

- `state.json` — `{dedup_key: first_seen_iso}`, plus per-source error-ping
  rate-limit timestamps
- `showings.json` — `{generated_at, sources: {cineplexx: "ok"|"error",
  megaplex: "ok"|"error"}, showings: [...]}`

## Telegram notification

- Bot created via BotFather; `TELEGRAM_BOT_TOKEN` and `TELEGRAM_CHAT_ID`
  come from a k8s Secret.
- One message per run, only if new showings were found; grouped by cinema,
  then date; German text; each showing with its booking link.
- Silent when nothing new.

## Error handling

- Sanity checks per source: response parses, expected fields present,
  plausible structure (e.g. Megaplex page contains film links / day groups).
- Failed sanity check → Telegram "source X looks broken" ping, rate-limited
  to 1 per day per source (timestamps in `state.json`).
- Network error: one retry, then the source counts as failed for the run.
- Corrupt state file: back it up next to the original, start empty (worst
  case: one re-notification of current showings).

## Deployment (k8s)

Manifests in `k8s/` (plain YAML, no Helm):

- `Deployment`: 1 replica, liveness+readiness probes against `/healthz`,
  PVC mounted at `/data`
- `Service`: ClusterIP
- `PersistentVolumeClaim`: 10Mi
- `Secret` (example file): `TELEGRAM_BOT_TOKEN`, `TELEGRAM_CHAT_ID`
- `ConfigMap`: `CHECK_INTERVAL_HOURS`, enabled cinemas
- Ingress: out of scope (user wires it to their cluster's ingress controller)

Image: `python:3.13-slim` + `pip install requests beautifulsoup4 flask`,
non-root user, built from a `Dockerfile` in the repo root.

## Testing

- TDD throughout.
- Parser tests use real fixtures captured from the live sites on 2026-07-18
  (Cineplexx API JSON incl. actual OV sessions; Megaplex HTML pages incl. an
  OV-filtered program page and an OV film page).
- Unit tests: both parsers, the OV matcher, dedup/pruning, Telegram message
  formatting (HTTP send mocked), Flask routes via the Flask test client.

## Repo layout

```
app/
  fetchers.py    # fetch_cineplexx(), fetch_megaplex() -> list[Showing]
  models.py      # Showing, dedup keys, version matching
  state.py       # load/save state.json + showings.json, pruning
  notify.py      # Telegram send + message formatting
  checker.py     # one check run: fetch -> diff -> notify -> write showings.json
  web.py         # Flask app (/, /healthz)
  main.py        # scheduler thread + web server entrypoint
tests/           # unit tests + fixtures
k8s/             # deployment, service, pvc, configmap, secret example
Dockerfile
requirements.txt
docs/superpowers/specs/2026-07-18-cinema-ov-watcher-design.md
```
