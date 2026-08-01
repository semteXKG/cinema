# Movie metadata (runtime, genre, poster) — design

Date: 2026-08-01

## Goal

Enrich every movie with **runtime, genre and a poster image**, sourced natively
from the two cinema sites (no external APIs, no extra HTTP requests), and show it
**everywhere**: web UI cards (poster + genre/runtime line), Telegram alert text,
and ICS event durations (replacing the fixed 2h guess). Posters are **cached
locally** — page loads never hit the cinema CDNs.

## Decisions (from brainstorming)

- **Native source parsing** — both cinemas already expose all three fields in
  responses we fetch today (verified live):
  - Cineplexx movie-list JSON: `runTime` (int minutes), `genres` (list),
    `posterImage` (URL).
  - Megaplex film page: `application/ld+json` block with `@type == "Movie"` →
    `duration` (`PT107M`), `genre` (list), `image` (list of URLs).
  - TMDB/OMDb enrichment is unnecessary (YAGNI).
- **Metadata lives in a separate `movies` map** in `showings.json`, keyed
  `"Cinema|Title"` (the exact `Showing.cinema` / `Showing.movie` strings). The
  flat `"showings"` list and the dedup-critical `Showing` dataclass stay
  byte-identical; consumers join on `f"{s.cinema}|{s.movie}"`.
- **Scope: everywhere** — web cards, Telegram text, ICS `DTEND`.
- **Card layout A** (chosen via visual mockup): 58px poster thumbnail left of the
  title block, `Genre · N Min` subtitle under the title; showings stay full-width
  below.
- **Posters cached locally** in `DATA_DIR/posters/`; web serves them itself.
- Everything metadata-related is **best-effort**: absence or parse failure never
  raises, never affects showings, alerts, or source health.

## Components

### `app/models.py`

New frozen dataclass:

```python
@dataclass(frozen=True)
class MovieMeta:
    runtime_min: int | None = None
    genres: tuple[str, ...] = ()
    poster: str | None = None  # remote URL, for cache (re)download
```

### `app/fetchers.py`

- `fetch_cineplexx` / `fetch_megaplex` return `(showings, metas)` where `metas:
  dict[str, MovieMeta]` is keyed `"Cinema|Title"` (each fetcher namespaces with
  its own cinema-name constant; parse helpers stay title-keyed).
- Cineplexx: build `MovieMeta` from each movie-list entry — `runTime` (missing
  or `0` → `None`), `genres`, `posterImage` (empty → `None`). Key uses the same
  cleaned title as `Showing.movie` (`title.lstrip("*").strip()`).
- Megaplex: in `parse_megaplex_film_page`, scan `<script type="application/ld+json">`
  blocks for `@type == "Movie"`; map `duration` via regex `PT(\d+)M` → int,
  `genre` list, `image[0]`. Missing/invalid block → no meta for that film; the
  page's showings still parse. Key uses the h1-derived title, as today.
- Existing parse signatures change shape `(showings, metas)` — update the
  fetcher-map lambdas in `checker.py` accordingly.

### `app/state.py`

- `movie_meta_to_dict(m: MovieMeta, poster_file: str | None = None) -> dict` →
  `{"runtime_min": ..., "genres": [...], "poster": ..., "poster_file": ...}` —
  `poster_file` is not part of `MovieMeta`; the checker passes the cached
  basename (or `None` when not cached).
- No changes to `save_showings`/`load_showings` — the `"movies"` key is just part
  of the payload dict.

### `app/checker.py`

- `run_check` collects each source's `metas` alongside showings; on source
  error both are absent (health logic unchanged).
- **Filter**: keep only metas whose key matches an upcoming OV showing
  (`{f"{s.cinema}|{s.movie}" for s in upcoming}`). Cineplexx's movie list covers
  all versions, not just OV — filtering avoids poster downloads for films we
  never display, and the map self-prunes as films leave the program.
- **Poster download**: for each filtered meta with a `poster` URL, compute
  `sha1(url)[:16] + ext` (ext from URL path, `.jpg`/`.jpeg`/`.png`/`.webp`,
  default `.jpg`). If absent from `DATA_DIR/posters/`, GET via the existing
  `HttpClient` and save atomically (tmp file + `replace`, mirroring `_save_json`).
  Failure → skip silently; retried next run.
- **Prune**: after building the payload, delete files in `posters/` not referenced
  by any current movie entry's `poster_file`.
- `save_showings` payload gains `"movies": {key: movie_meta_to_dict(...)}`.

### `app/web.py`

- New route `GET /posters/<name>` → `send_from_directory(Path(data_dir) /
  "posters", name)` with `Cache-Control: public, max-age=86400`; 404 if missing.
- Card template: when the movie's meta has `poster_file`, render
  `<img src="/posters/<file>" loading="lazy" alt="">` (58px, rounded, bordered) in
  a flex row left of the title block; otherwise render today's title-only layout.
- Subtitle line under the title: genres joined with `, `, then `· N Min` when
  runtime known — whichever parts exist; line hidden when neither.
- Missing `"movies"` key or missing entry → exactly today's rendering.

### `app/notify.py`

- Movie title line gains a suffix: `<b>Title (OV)</b> — Komödie, 107 Min`. The
  suffix is independent of the existing version rendering (which only shows
  `(OV)` when all of the movie's showings share one version) — meta appends to
  the title line in both cases; parts omitted when unknown (line unchanged when
  no meta). German `Min` matches the existing message style. No images in Telegram.
- `format_message` gains an optional `movies: dict | None = None` param
  (default keeps current behavior).

### `app/ics.py`

- `render_ics(showings, now=None, *, movies: dict | None = None)` — `movies` is
  keyword-only, so all existing callers (which already pass `now=` by keyword)
  keep working. When the showing's movie entry has `runtime_min`,
  `DTEND = start + runtime`; otherwise the fixed 2h fallback stays.
- This fulfills the old "real runtimes" item from the ICS spec's out-of-scope list.

## Data flow

`fetch_*` (showings + metas) → `run_check` (merge metas, download/prune posters,
write `showings.json` with `"movies"` section) → consumers join on
`cinema|movie`: web (`/` and `/posters/<name>`), Telegram (`format_message`),
ICS (`render_ics`).

`DATA_DIR/posters/` is created on demand; it is transient cache (safe to delete —
re-downloaded on next check).

## Error handling

- Metadata parsing is best-effort everywhere: unknown/malformed fields →
  `None`/absent, never an exception. A Megaplex page without JSON-LD is not a
  `SourceError`.
- Poster download failure → no `poster_file`; web renders without image (never
  the remote URL).
- Corrupt/missing `posters/` dir → recreated on demand; 404s until re-cached.
- Old `showings.json` (no `"movies"` key) → all consumers behave as today.

## Testing

- `test_fetchers_cineplexx.py` — meta extraction from fixture (runtime, genres,
  poster; `runTime: 0` → `None`); returned tuple shape.
- `test_fetchers_megaplex.py` — JSON-LD meta extraction; page without JSON-LD →
  showings parse, no meta.
- `test_checker.py` — payload contains `"movies"`; poster download called for new
  URL, skipped when file exists; failed download → no `poster_file`; prune removes
  unreferenced files.
- `test_web.py` — card shows poster `img` + subtitle with meta; renders fine
  without; `/posters/<name>` serves file (200, cache header) and 404s for missing.
- `test_notify.py` — title line suffix with meta; unchanged without.
- `test_ics.py` — `DTEND` = start + runtime with meta; 2h fallback without.

## Out of scope

- TMDB/OMDb or any external metadata API.
- Images in Telegram messages.
- Cross-cinema movie merging (each cinema's entry keeps its own metadata).
- Synopsis/director/cast/FSK (available from both sources; not requested).
