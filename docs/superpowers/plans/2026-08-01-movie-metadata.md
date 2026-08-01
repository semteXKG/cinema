# Movie Metadata (Runtime, Genre, Poster) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enrich every movie with runtime, genre and a locally-cached poster — parsed natively from both cinema sites — and surface it in the web UI, Telegram alerts, and ICS event durations.

**Architecture:** Fetchers return `(showings, metas)` where `metas` maps `"Cinema|Title"` → `MovieMeta`. The checker filters metas to movies with upcoming OV showings, downloads posters into `DATA_DIR/posters/`, and writes a `"movies"` section into `showings.json`. Web, Telegram and ICS consumers join on `cinema|movie`. The `Showing` dataclass and the flat `"showings"` list are untouched.

**Tech Stack:** Python 3.13, Flask, requests, BeautifulSoup, pytest. No new dependencies.

## Global Constraints

- No new dependencies (`requirements.txt` stays flask+requests+beautifulsoup4).
- Metadata is strictly best-effort: parse/download problems never raise, never affect showings, dedup, alerts, or source health.
- `Showing` dataclass, its `key`, and the `"showings"` list in `showings.json` stay byte-identical.
- Movies map key = `f"{Showing.cinema}|{Showing.movie}"` (exact strings).
- Posters live under `DATA_DIR/posters/`; the web UI never hotlinks remote poster URLs.
- Filename = `sha1(url)[:16] + ext`, ext ∈ `.jpg/.jpeg/.png/.webp` (from URL path, default `.jpg`).
- Old `showings.json` without a `"movies"` key → all consumers behave exactly as today.
- Runtime rendered as `N Min` (web and Telegram); genres joined with `, `.
- Commit style: plain imperative (e.g. `Add poster cache to checker`), matching repo history.

---

### Task 1: `MovieMeta` model + `movie_meta_to_dict`

**Files:**
- Modify: `app/models.py` (append at end)
- Modify: `app/state.py` (import + append)
- Test: `tests/test_models.py`, `tests/test_state.py`

**Interfaces:**
- Produces: `MovieMeta(runtime_min: int | None = None, genres: tuple[str, ...] = (), poster: str | None = None)` — frozen dataclass; `poster` is the remote URL used for cache (re)download.
- Produces: `app.state.movie_meta_to_dict(m: MovieMeta, poster_file: str | None = None) -> dict` → `{"runtime_min": int|None, "genres": list[str], "poster": str|None, "poster_file": str|None}` — the JSON shape of every `"movies"` entry.

- [ ] **Step 1: Write the failing tests**

Append to `tests/test_models.py` (add `MovieMeta` to the existing `from app.models import (...)` list):

```python
def test_movie_meta_defaults():
    m = MovieMeta()
    assert m.runtime_min is None
    assert m.genres == ()
    assert m.poster is None


def test_movie_meta_is_frozen():
    import dataclasses

    m = MovieMeta(180, ("Drama",), "https://x/p.jpg")
    with dataclasses.FrozenInstanceError:
        m.runtime_min = 90  # type: ignore[misc]
```

Append to `tests/test_state.py` (add `movie_meta_to_dict` to the existing `from app.state import (...)` list, and `from app.models import MovieMeta, Showing` — extend the existing models import):

```python
def test_movie_meta_to_dict():
    d = movie_meta_to_dict(
        MovieMeta(180, ("Drama", "Action"), "https://x/p.jpg"), "abc123.jpg"
    )
    assert d == {
        "runtime_min": 180,
        "genres": ["Drama", "Action"],
        "poster": "https://x/p.jpg",
        "poster_file": "abc123.jpg",
    }
    assert json.dumps(d)  # serializable


def test_movie_meta_to_dict_defaults():
    d = movie_meta_to_dict(MovieMeta())
    assert d == {
        "runtime_min": None,
        "genres": [],
        "poster": None,
        "poster_file": None,
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `.venv/bin/python -m pytest tests/test_models.py tests/test_state.py -v`
Expected: FAIL — `ImportError: cannot import name 'MovieMeta'`

- [ ] **Step 3: Write the implementation**

Append to `app/models.py`:

```python
@dataclass(frozen=True)
class MovieMeta:
    """Per-movie metadata, keyed 'Cinema|Title' in the showings.json movies map."""

    runtime_min: int | None = None
    genres: tuple[str, ...] = ()
    poster: str | None = None  # remote URL, for cache (re)download
```

In `app/state.py`, extend the import `from .models import Showing` to `from .models import MovieMeta, Showing`, then append:

```python
def movie_meta_to_dict(m: MovieMeta, poster_file: str | None = None) -> dict:
    return {
        "runtime_min": m.runtime_min,
        "genres": list(m.genres),
        "poster": m.poster,
        "poster_file": poster_file,
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `.venv/bin/python -m pytest tests/test_models.py tests/test_state.py -v`
Expected: PASS (all, including pre-existing)

- [ ] **Step 5: Commit**

```bash
git add app/models.py app/state.py tests/test_models.py tests/test_state.py
git commit -m "Add MovieMeta model and movie_meta_to_dict serializer"
```

---

### Task 2: Fetchers return `(showings, metas)` + checker writes `"movies"` section

**Files:**
- Modify: `app/fetchers.py` (imports, both `fetch_*`, both parse functions)
- Modify: `app/checker.py` (unpack tuple, build movie dicts, payload key)
- Test: `tests/test_fetchers_cineplexx.py`, `tests/test_fetchers_megaplex.py`, `tests/test_fetchers_http.py`, `tests/test_checker.py`

**Interfaces:**
- Consumes: `MovieMeta`, `movie_meta_to_dict` from Task 1.
- Produces: `parse_cineplexx_showings(movies, sessions_by_movie) -> tuple[list[Showing], dict[str, MovieMeta]]` (metas keyed by cleaned title, one per movie-list entry).
- Produces: `fetch_cineplexx(http) -> tuple[list[Showing], dict[str, MovieMeta]]` (metas keyed `"Cineplexx Linz|Title"`).
- Produces: `parse_megaplex_film_page(html, url, today) -> tuple[list[Showing], dict[str, MovieMeta]]` (metas keyed by h1 title, empty when no JSON-LD).
- Produces: `fetch_megaplex(http, today) -> tuple[list[Showing], dict[str, MovieMeta]]` (metas keyed `"Megaplex PlusCity|Title"`).
- Produces: `showings.json` payload gains `"movies": {key: movie_meta_to_dict(m)}` with `poster_file: None` for now (Task 3 fills it).

**Note:** this task deliberately changes the fetcher return type and the checker's consumption in the same task — splitting them would leave the suite red in between.

- [ ] **Step 1: Update/extend the fetcher tests (failing)**

`tests/test_fetchers_cineplexx.py` — unpack tuples in the three existing tests (`showings, _ = parse_cineplexx_showings(...)` etc.) and append:

```python
def test_extracts_movie_metadata():
    movies, sessions = load()
    _, metas = parse_cineplexx_showings(movies, sessions)
    m = metas["The Odyssey"]
    assert m.runtime_min == 180
    assert m.genres == ("Abenteuer", "Historie")
    assert m.poster and m.poster.startswith("https://")


def test_metas_cover_all_movies_even_without_ov_sessions():
    movies, sessions = load()
    _, metas = parse_cineplexx_showings(movies, sessions)
    # one entry per movie in the list (17); checker filters to OV showings later
    assert len(metas) == len(movies) == 17


def test_cineplexx_meta_edge_values():
    # runTime 0 / missing genres / empty poster all degrade to None/()
    movies = [{"id": "X1", "title": "Odd", "runTime": 0,
               "genres": None, "posterImage": ""}]
    _, metas = parse_cineplexx_showings(movies, {})
    assert metas["Odd"] == MovieMeta(None, (), None)
```

(The last test also needs `from app.models import MovieMeta` at the top of the file.)

`tests/test_fetchers_megaplex.py` — unpack tuples in the two film-page tests and append:

```python
def test_parse_film_page_metadata():
    html = load_fixture("megaplex_film_ov.html")
    url = f"{MEGAPLEX_BASE}/film/linz/die-odyssee/ov"
    _, metas = parse_megaplex_film_page(html, url, TODAY)
    m = metas["Die Odyssee"]
    assert m.runtime_min == 173  # JSON-LD duration "PT173M"
    assert m.genres == ("Drama", "Action", "Abenteuer", "Fantasy")
    assert m.poster == "https://megaplexog.s3.eu-north-1.amazonaws.com/Odysee1.webp"


def test_parse_film_page_without_jsonld_has_no_meta():
    html = (
        "<html><body><h1>Other (Pluscity) - OV</h1>"
        "Aktuelles Kinoprogramm</body></html>"
    )
    showings, metas = parse_megaplex_film_page(html, "https://x", TODAY)
    assert showings == []
    assert metas == {}
```

`tests/test_fetchers_http.py` — update the two happy-path tests:

```python
def test_fetch_cineplexx_returns_ov_showings():
    showings, metas = fetch_cineplexx(make_cineplexx_http())
    assert len(showings) == 6
    assert all(s.cinema == "Cineplexx Linz" for s in showings)
    meta = metas["Cineplexx Linz|The Odyssey"]
    assert meta.runtime_min == 180
    # namespaced with the cinema name
    assert all(k.startswith("Cineplexx Linz|") for k in metas)
```

```python
def test_fetch_megaplex_returns_ov_showings():
    http = FakeHttp()
    program = load_fixture("megaplex_ov_program.html")
    film = load_fixture("megaplex_film_ov.html")
    http.text_routes["/kinoprogramm/linz/"] = program
    http.text_routes["/film/linz/die-odyssee/ov"] = film
    # other linked OV films (insekten, vaiana) return a valid page without day-groups
    http.text_routes["/film/linz/"] = (
        "<html><body><h1>Other (Pluscity) - OV</h1>"
        "Aktuelles Kinoprogramm</body></html>"
    )
    showings, metas = fetch_megaplex(http, TODAY)
    assert len(showings) == 8
    assert all(s.cinema == "Megaplex PlusCity" for s in showings)
    # 14 program pages (one per day) + film pages
    program_calls = [u for u in http.requested if "/kinoprogramm/linz/" in u]
    assert len(program_calls) == 14
    # only the odyssey page carries JSON-LD metadata
    assert list(metas) == ["Megaplex PlusCity|Die Odyssee"]
    assert metas["Megaplex PlusCity|Die Odyssee"].runtime_min == 173
```

- [ ] **Step 2: Update checker tests (failing)**

`tests/test_checker.py` — `FakeFetcher` must return tuples; extend it and add one test:

```python
class FakeFetcher:
    def __init__(self, showings=None, metas=None, error=None):
        self.showings = showings or []
        self.metas = metas or {}
        self.error = error

    def __call__(self, http, today=None):
        if self.error:
            raise self.error
        return self.showings, self.metas
```

Append (add `from app.models import MovieMeta, Showing` — extend the existing models import):

```python
def test_showings_json_contains_movies_filtered_to_shown(tmp_path):
    metas = {
        "Cineplexx Linz|The Odyssey": MovieMeta(
            180, ("Abenteuer", "Historie"), "https://x/p.jpg"
        ),
        "Cineplexx Linz|Not Shown": MovieMeta(90, ("Drama",), "https://x/q.jpg"),
    }
    fetchers = {"cineplexx": FakeFetcher([make_showing()], metas=metas)}
    cfg = Config(telegram_token=None, telegram_chat_id=None, sources=("cineplexx",))
    run_check(None, tmp_path, cfg, NOW, fetcher_map=fetchers)
    movies = load_showings(tmp_path)["movies"]
    # meta for a movie without upcoming showings is filtered out
    assert list(movies) == ["Cineplexx Linz|The Odyssey"]
    assert movies["Cineplexx Linz|The Odyssey"] == {
        "runtime_min": 180,
        "genres": ["Abenteuer", "Historie"],
        "poster": "https://x/p.jpg",
        "poster_file": None,  # filled by the poster cache (Task 3)
    }
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `.venv/bin/python -m pytest tests/test_fetchers_cineplexx.py tests/test_fetchers_megaplex.py tests/test_fetchers_http.py tests/test_checker.py -v`
Expected: FAIL — unpack errors / `ValueError: too many values to unpack` / TypeErrors, because parse functions still return bare lists.

- [ ] **Step 4: Implement fetcher changes**

In `app/fetchers.py`:

1. Add `import json` to the stdlib imports and extend `from .models import ...` to include `MovieMeta`:

```python
from .models import MovieMeta, Showing, cineplexx_session_version, megaplex_version
```

2. Add the two meta helpers (place near the parse functions):

```python
def _cineplexx_meta(movie: dict) -> MovieMeta:
    runtime = movie.get("runTime") or None  # missing or 0 -> None
    genres = tuple(g for g in (movie.get("genres") or []) if isinstance(g, str))
    poster = movie.get("posterImage") or None
    return MovieMeta(runtime, genres, poster)


_LD_DURATION_RE = re.compile(r"PT(?:(\d+)H)?(?:(\d+)M)?")


def _megaplex_jsonld_meta(soup: BeautifulSoup) -> MovieMeta | None:
    """Extract runtime/genre/poster from the schema.org Movie JSON-LD block."""
    for tag in soup.find_all("script", type="application/ld+json"):
        try:
            data = json.loads(tag.string or "")
        except ValueError:
            continue
        blocks = data if isinstance(data, list) else [data]
        for block in blocks:
            if not isinstance(block, dict) or block.get("@type") != "Movie":
                continue
            runtime = None
            m = _LD_DURATION_RE.fullmatch(str(block.get("duration") or ""))
            if m:
                runtime = int(m.group(1) or 0) * 60 + int(m.group(2) or 0)
                runtime = runtime or None
            genre = block.get("genre") or []
            if isinstance(genre, str):
                genre = [genre]
            image = block.get("image") or []
            if isinstance(image, str):
                image = [image]
            return MovieMeta(
                runtime_min=runtime,
                genres=tuple(g for g in genre if isinstance(g, str)),
                poster=image[0] if image else None,
            )
    return None
```

3. `parse_cineplexx_showings` — build metas alongside showings, change return:

```python
def parse_cineplexx_showings(
    movies: list[dict], sessions_by_movie: dict[str, list]
) -> tuple[list[Showing], dict[str, MovieMeta]]:
    showings = []
    metas: dict[str, MovieMeta] = {}
    for movie in movies:
        title = (movie.get("title") or "").lstrip("*").strip()
        metas.setdefault(title, _cineplexx_meta(movie))
        url = f"https://cineplexx.at/film/{movie.get('shortURL', '')}"
        for group in sessions_by_movie.get(movie.get("id"), []):
            for session in group.get("sessions", []):
                if session.get("cinemaId") != CINEPLEXX_CINEMA_ID:
                    continue
                version = cineplexx_session_version(session)
                if not version:
                    continue
                showings.append(
                    Showing(
                        cinema=CINEPLEXX_CINEMA_NAME,
                        movie=title,
                        start=datetime.fromisoformat(session["showtime"]),
                        version=version,
                        hall=session.get("screenName") or "",
                        url=url,
                    )
                )
    showings.sort(key=lambda s: s.start)
    return showings, metas
```

4. `fetch_cineplexx` — namespace meta keys with the cinema name:

```python
def fetch_cineplexx(http) -> tuple[list[Showing], dict[str, MovieMeta]]:
    movies = http.get_json(
        f"{CINEPLEXX_BASE}/api/v1/cinemasweb/{CINEPLEXX_CINEMA_ID}/movies",
        headers=CINEPLEXX_HEADERS,
        params={"date": "all"},
    )
    if not isinstance(movies, list) or not movies:
        raise SourceError("Cineplexx: empty or invalid movie list")
    sessions_by_movie = {}
    for movie in movies:
        data = http.get_json(
            f"{CINEPLEXX_BASE}/api/v2/moviesweb/{movie['id']}/sessions",
            headers=CINEPLEXX_HEADERS,
            params={"location": "AUT"},
        )
        if not isinstance(data, list):
            raise SourceError(f"Cineplexx: invalid sessions for {movie['id']}")
        sessions_by_movie[movie["id"]] = data
    showings, metas = parse_cineplexx_showings(movies, sessions_by_movie)
    return showings, {
        f"{CINEPLEXX_CINEMA_NAME}|{title}": meta for title, meta in metas.items()
    }
```

5. `fetch_megaplex` — collect and namespace metas across film pages:

```python
def fetch_megaplex(http, today: date) -> tuple[list[Showing], dict[str, MovieMeta]]:
    links: list[str] = []
    for i in range(MEGAPLEX_DAYS):
        day = today + timedelta(days=i)
        html = http.get_text(
            f"{MEGAPLEX_BASE}/kinoprogramm/linz/{day.isoformat()}/ov",
            headers=MEGAPLEX_HEADERS,
        )
        if "Kinoprogramm" not in html:
            raise SourceError(f"Megaplex: unexpected program page for {day}")
        for url in parse_megaplex_ov_links(html):
            if url not in links:
                links.append(url)
    showings: list[Showing] = []
    metas: dict[str, MovieMeta] = {}
    for url in links:
        html = http.get_text(url, headers=MEGAPLEX_HEADERS)
        page_showings, page_metas = parse_megaplex_film_page(html, url, today)
        showings.extend(page_showings)
        for title, meta in page_metas.items():
            metas.setdefault(f"{MEGAPLEX_CINEMA_NAME}|{title}", meta)
    return showings, metas
```

6. `parse_megaplex_film_page` — full new version (signature, meta block, and return change; the `day-group` loop body is unchanged):

```python
def parse_megaplex_film_page(
    html: str, url: str, today: date
) -> tuple[list[Showing], dict[str, MovieMeta]]:
    soup = BeautifulSoup(html, "html.parser")
    if "Kinoprogramm" not in soup.get_text():
        raise SourceError(f"unexpected Megaplex film page: {url}")
    h1 = soup.find("h1")
    title = ""
    if h1:
        raw = h1.get_text(" ", strip=True)
        title = re.split(r"\s*\(Pluscity\)|\s+-\s+OV", raw)[0].strip()
    metas: dict[str, MovieMeta] = {}
    meta = _megaplex_jsonld_meta(soup)
    if meta and title:
        metas[title] = meta
    showings: list[Showing] = []
    for group in soup.select("div.day-group"):
        h3 = group.find("h3")
        day = _parse_day(h3.get_text(" ", strip=True), today) if h3 else None
        if not day:
            continue
        for a in group.select("a.card-highlights-link"):
            label_el = a.select_one(".card-highlights-content-time-kino")
            version = (
                megaplex_version(label_el.get_text(" ", strip=True))
                if label_el
                else None
            )
            if not version:
                continue
            tm = _TIME_RE.match(a.get("title", ""))
            if not tm:
                continue
            start = datetime(
                day.year, day.month, day.day,
                int(tm.group(1)), int(tm.group(2)), tzinfo=_TZ,
            )
            href = a.get("href", "")
            full_url = MEGAPLEX_BASE + href if href.startswith("/") else href
            showings.append(
                Showing(
                    cinema=MEGAPLEX_CINEMA_NAME,
                    movie=title,
                    start=start,
                    version=version,
                    hall="",
                    url=full_url,
                )
            )
    showings.sort(key=lambda s: s.start)
    return showings, metas
```

- [ ] **Step 5: Implement checker changes**

In `app/checker.py`, inside `run_check`:

1. Declare the accumulator next to `all_showings`:

```python
    all_showings: list[Showing] = []
    all_metas: dict[str, MovieMeta] = {}
```

2. Replace `all_showings.extend(fetcher_map[source](http, now.date()))` with:

```python
            showings, metas = fetcher_map[source](http, now.date())
            all_showings.extend(showings)
            all_metas.update(metas)
```

3. Extend the import: `from .models import MovieMeta, Showing`.

4. Just before the `state_mod.save_showings(...)` call, insert:

```python
    wanted = {f"{s.cinema}|{s.movie}" for s in upcoming}
    movie_dicts = {
        key: state_mod.movie_meta_to_dict(m)
        for key, m in all_metas.items()
        if key in wanted
    }
```

5. Add `"movies": movie_dicts,` to the `save_showings` payload dict (after `"sources": health,`).

- [ ] **Step 6: Run the full suite**

Run: `.venv/bin/python -m pytest -q`
Expected: PASS — all tests, old and new.

- [ ] **Step 7: Commit**

```bash
git add app/fetchers.py app/checker.py tests/test_fetchers_cineplexx.py tests/test_fetchers_megaplex.py tests/test_fetchers_http.py tests/test_checker.py
git commit -m "Extract movie metadata from both sources into showings.json"
```

---

### Task 3: Poster cache in the checker

**Files:**
- Modify: `app/fetchers.py` (`HttpClient.get_bytes`)
- Modify: `app/checker.py` (filename helper, download, prune, wire into `run_check`)
- Test: `tests/test_fetchers_http.py`, `tests/test_checker.py`

**Interfaces:**
- Consumes: Task 2's `all_metas` (filtered) and `movie_dicts` construction.
- Produces: `HttpClient.get_bytes(url, headers=None) -> bytes`.
- Produces: `showings.json` movie entries with `poster_file` = cached basename or `None`; poster files under `DATA_DIR/posters/`; unreferenced files pruned each run.

- [ ] **Step 1: Write the failing tests**

Append to `tests/test_fetchers_http.py` (add `HttpClient` to the fetchers import):

```python
def test_http_client_get_bytes(monkeypatch):
    class Resp:
        content = b"\xff\xd8img"

        def raise_for_status(self):
            pass

    client = HttpClient()
    monkeypatch.setattr(client._session, "get", lambda *a, **k: Resp())
    assert client.get_bytes("https://cdn.example/p.jpg") == b"\xff\xd8img"
```

Append to `tests/test_checker.py`:

```python
class FakePosterHttp:
    """Serves poster bytes; content=None simulates a download failure."""

    def __init__(self, content=b"\xff\xd8img"):
        self.content = content
        self.requested = []

    def get_bytes(self, url, headers=None):
        self.requested.append(url)
        if self.content is None:
            raise RuntimeError("download failed")
        return self.content


def make_meta(poster="https://cdn.example/poster.jpg"):
    return MovieMeta(180, ("Abenteuer", "Historie"), poster)


def test_posters_downloaded_and_referenced(tmp_path):
    http = FakePosterHttp()
    metas = {
        "Cineplexx Linz|The Odyssey": make_meta(),
        # filtered out (no such showing) -> must not trigger a download
        "Cineplexx Linz|Not Shown": make_meta("https://cdn.example/other.png"),
    }
    fetchers = {"cineplexx": FakeFetcher([make_showing()], metas=metas)}
    cfg = Config(telegram_token=None, telegram_chat_id=None, sources=("cineplexx",))
    run_check(http, tmp_path, cfg, NOW, fetcher_map=fetchers)
    assert http.requested == ["https://cdn.example/poster.jpg"]
    files = list((tmp_path / "posters").iterdir())
    assert len(files) == 1
    assert files[0].read_bytes() == b"\xff\xd8img"
    entry = load_showings(tmp_path)["movies"]["Cineplexx Linz|The Odyssey"]
    assert entry["poster_file"] == files[0].name
    # stable content-derived name: sha1(url)[:16] + ext
    import hashlib

    assert files[0].name == (
        hashlib.sha1(b"https://cdn.example/poster.jpg").hexdigest()[:16] + ".jpg"
    )


def test_poster_cache_hit_skips_download(tmp_path):
    metas = {"Cineplexx Linz|The Odyssey": make_meta()}
    fetchers = {"cineplexx": FakeFetcher([make_showing()], metas=metas)}
    cfg = Config(telegram_token=None, telegram_chat_id=None, sources=("cineplexx",))
    run_check(FakePosterHttp(), tmp_path, cfg, NOW, fetcher_map=fetchers)
    # second run with a fresh http: file exists -> no download
    http2 = FakePosterHttp()
    run_check(http2, tmp_path, cfg, NOW, fetcher_map=fetchers)
    assert http2.requested == []
    entry = load_showings(tmp_path)["movies"]["Cineplexx Linz|The Odyssey"]
    assert entry["poster_file"] is not None


def test_poster_download_failure_is_best_effort(tmp_path):
    http = FakePosterHttp(content=None)
    metas = {"Cineplexx Linz|The Odyssey": make_meta()}
    fetchers = {"cineplexx": FakeFetcher([make_showing()], metas=metas)}
    cfg = Config(telegram_token=None, telegram_chat_id=None, sources=("cineplexx",))
    run_check(http, tmp_path, cfg, NOW, fetcher_map=fetchers)  # must not raise
    entry = load_showings(tmp_path)["movies"]["Cineplexx Linz|The Odyssey"]
    assert entry["poster_file"] is None
    assert not list((tmp_path / "posters").iterdir())


def test_prune_removes_unreferenced_posters(tmp_path):
    posters = tmp_path / "posters"
    posters.mkdir()
    (posters / "stale.jpg").write_bytes(b"old")
    metas = {"Cineplexx Linz|The Odyssey": make_meta()}
    fetchers = {"cineplexx": FakeFetcher([make_showing()], metas=metas)}
    cfg = Config(telegram_token=None, telegram_chat_id=None, sources=("cineplexx",))
    run_check(FakePosterHttp(), tmp_path, cfg, NOW, fetcher_map=fetchers)
    names = {f.name for f in posters.iterdir()}
    assert "stale.jpg" not in names
    assert len(names) == 1
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `.venv/bin/python -m pytest tests/test_fetchers_http.py tests/test_checker.py -v`
Expected: FAIL — `AttributeError: 'HttpClient' object has no attribute 'get_bytes'`; `poster_file` still `None` / no `posters` dir.

- [ ] **Step 3: Implement `HttpClient.get_bytes`**

In `app/fetchers.py`, add to `HttpClient` (next to `get_text`):

```python
    def get_bytes(self, url, headers=None) -> bytes:
        return self._get(url, headers=headers, params=None).content
```

- [ ] **Step 4: Implement the poster cache in the checker**

In `app/checker.py`:

1. Extend imports:

```python
import hashlib
from urllib.parse import urlsplit
```

(`Path` is already imported.)

2. Add module-level constants and helpers (after `default_fetcher_map`):

```python
_POSTER_HEADERS = {
    "User-Agent": (
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
        "(KHTML, like Gecko) Chrome/120.0 Safari/537.36"
    ),
}
_POSTER_EXTS = {".jpg", ".jpeg", ".png", ".webp"}


def _poster_filename(url: str) -> str:
    ext = Path(urlsplit(url).path).suffix.lower()
    if ext not in _POSTER_EXTS:
        ext = ".jpg"
    return hashlib.sha1(url.encode("utf-8")).hexdigest()[:16] + ext


def _cache_posters(http, metas: dict[str, MovieMeta], posters_dir: Path) -> dict:
    """Download missing posters; return key -> cached basename (None if missing)."""
    cached: dict[str, str | None] = {}
    for key, meta in metas.items():
        if not meta.poster:
            cached[key] = None
            continue
        name = _poster_filename(meta.poster)
        target = posters_dir / name
        if not target.exists():
            try:
                posters_dir.mkdir(parents=True, exist_ok=True)
                content = http.get_bytes(meta.poster, headers=_POSTER_HEADERS)
            except Exception:  # best-effort: retry on the next run
                cached[key] = None
                continue
            tmp = target.with_suffix(target.suffix + ".tmp")
            tmp.write_bytes(content)
            tmp.replace(target)
        cached[key] = name
    return cached


def _prune_posters(posters_dir: Path, keep: set[str]) -> None:
    if not posters_dir.is_dir():
        return
    for f in posters_dir.iterdir():
        if f.is_file() and f.name not in keep:
            f.unlink()
```

3. In `run_check`, replace the `wanted`/`movie_dicts` block from Task 2 with the following, placed **before** the `if new and can_notify:` block (downloads happen before alerting, so a slow CDN delays the ping, not the state):

```python
    wanted = {f"{s.cinema}|{s.movie}" for s in upcoming}
    filtered_metas = {k: m for k, m in all_metas.items() if k in wanted}
    posters_dir = data_dir / "posters"
    poster_files = _cache_posters(http, filtered_metas, posters_dir)
    _prune_posters(posters_dir, {n for n in poster_files.values() if n})
    movie_dicts = {
        key: state_mod.movie_meta_to_dict(m, poster_files.get(key))
        for key, m in filtered_metas.items()
    }
```

- [ ] **Step 5: Run the full suite**

Run: `.venv/bin/python -m pytest -q`
Expected: PASS — including the Task 2 test whose `poster_file is None` assertion must still hold only when no http is usable. Note: `test_showings_json_contains_movies_filtered_to_shown` passes `run_check(None, ...)` — `None.get_bytes` raises `AttributeError`, caught by the blanket `except Exception`, so `poster_file` stays `None` there. Verify this explicitly.

- [ ] **Step 6: Commit**

```bash
git add app/fetchers.py app/checker.py tests/test_fetchers_http.py tests/test_checker.py
git commit -m "Cache movie posters locally under DATA_DIR/posters"
```

---

### Task 4: Web UI — `/posters/<name>` route + poster/meta on movie cards

**Files:**
- Modify: `app/web.py` (imports, `_group_showings`, `/posters` route, template + CSS)
- Test: `tests/test_web.py`

**Interfaces:**
- Consumes: `showings.json` `"movies"` section (`poster_file`, `genres`, `runtime_min`).
- Produces: `GET /posters/<name>` → file bytes (200, `Cache-Control: max-age=86400`) or 404.
- Produces: `_group_showings(showings: list[dict], movies: dict | None = None) -> list[dict]` — each movie dict gains `poster: str | None` and `meta_line: str`.

- [ ] **Step 1: Write the failing tests**

In `tests/test_web.py`, extend `write_payload` to take an optional movies map:

```python
def write_payload(data_dir, showings, movies=None):
    payload = {
        "generated_at": datetime(2026, 7, 18, 12, 0, tzinfo=TZ).isoformat(),
        "sources": {"cineplexx": "ok", "megaplex": "error"},
        "showings": showings,
    }
    if movies is not None:
        payload["movies"] = movies
    save_showings(data_dir, payload)
```

Append:

```python
ODYSSEY_SHOWING = {
    "cinema": "Cineplexx Linz", "movie": "The Odyssey",
    "start": "2026-07-20T19:00:00+02:00", "version": "OV",
    "hall": "Saal 7", "url": "https://cineplexx.at/film/die-odyssee",
}

ODYSSEY_META = {
    "runtime_min": 180, "genres": ["Abenteuer", "Historie"],
    "poster": "https://x/p.jpg", "poster_file": "abc123.jpg",
}


def test_card_shows_poster_and_meta_line(tmp_path):
    write_payload(tmp_path, [ODYSSEY_SHOWING],
                  movies={"Cineplexx Linz|The Odyssey": ODYSSEY_META})
    html = create_app(tmp_path).test_client().get("/").data.decode()
    assert '<img src="/posters/abc123.jpg"' in html
    assert 'loading="lazy"' in html
    assert "Abenteuer, Historie · 180 Min" in html


def test_card_meta_line_runtime_only(tmp_path):
    meta = {**ODYSSEY_META, "genres": [], "poster_file": None}
    write_payload(tmp_path, [ODYSSEY_SHOWING],
                  movies={"Cineplexx Linz|The Odyssey": meta})
    html = create_app(tmp_path).test_client().get("/").data.decode()
    assert "180 Min" in html
    assert "· 180 Min" not in html  # no dangling separator without genres
    assert "<img" not in html  # no poster_file -> no image, no remote hotlink


def test_card_without_movies_section_renders_as_before(tmp_path):
    write_payload(tmp_path, [ODYSSEY_SHOWING])
    html = create_app(tmp_path).test_client().get("/").data.decode()
    assert "The Odyssey" in html
    assert "<img" not in html
    assert "Min" not in html


def test_poster_route_serves_cached_file(tmp_path):
    (tmp_path / "posters").mkdir()
    (tmp_path / "posters" / "abc123.jpg").write_bytes(b"\xff\xd8")
    resp = create_app(tmp_path).test_client().get("/posters/abc123.jpg")
    assert resp.status_code == 200
    assert resp.data == b"\xff\xd8"
    assert "max-age=86400" in resp.headers["Cache-Control"]


def test_poster_route_404_for_missing_file(tmp_path):
    resp = create_app(tmp_path).test_client().get("/posters/nope.jpg")
    assert resp.status_code == 404
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `.venv/bin/python -m pytest tests/test_web.py -v`
Expected: FAIL — no `<img`, no meta line, `/posters/...` 404s.

- [ ] **Step 3: Implement web changes**

In `app/web.py`:

1. Imports:

```python
from pathlib import Path

from flask import Flask, Response, render_template_string, send_from_directory
```

2. Add the meta-line helper above `_group_showings`:

```python
def _meta_line(meta: dict) -> str:
    parts = []
    genres = [g for g in meta.get("genres") or [] if isinstance(g, str)]
    if genres:
        parts.append(", ".join(genres))
    if meta.get("runtime_min"):
        parts.append(f"{meta['runtime_min']} Min")
    return " · ".join(parts)
```

3. Replace `_group_showings` (note: the inner list is renamed to `movie_cards` to free the name `movies` for the new parameter):

```python
def _group_showings(showings: list[dict], movies: dict | None = None) -> list[dict]:
    """Group flat showing dicts into cinema -> movie -> showing rows."""
    metas = movies or {}
    parsed = sorted(
        ((datetime.fromisoformat(s["start"]), s) for s in showings),
        key=lambda x: x[0],
    )
    by_cinema: dict[str, dict[str, list]] = defaultdict(lambda: defaultdict(list))
    for start, s in parsed:
        by_cinema[s["cinema"]][s["movie"]].append((start, s))

    cinemas = []
    for cinema in sorted(by_cinema, key=_cinema_key):
        movie_cards = []
        for movie, entries in by_cinema[cinema].items():
            meta = metas.get(f"{cinema}|{movie}") or {}
            bases = {s["version"].split(" - ")[0].strip() for _, s in entries}
            badge = next(iter(bases)) if len(bases) == 1 else None
            rows = []
            for start, s in entries:
                parts = []
                variant = _short_version(s["version"])
                if badge is None:
                    parts.append(variant or s["version"])
                elif variant:
                    parts.append(variant)
                if s.get("hall"):
                    parts.append(s["hall"])
                rows.append(
                    {
                        "date": f"{_WEEKDAYS[start.weekday()]} {start:%d.%m}.",
                        "time": f"{start:%H:%M}",
                        "detail": ", ".join(parts),
                        "url": s["url"],
                    }
                )
            movie_cards.append(
                (
                    entries[0][0],
                    {
                        "movie": movie,
                        "badge": badge,
                        "showings": rows,
                        "poster": meta.get("poster_file"),
                        "meta_line": _meta_line(meta),
                    },
                )
            )
        movie_cards.sort(key=lambda t: t[0])
        cinemas.append({"name": cinema, "movies": [m for _, m in movie_cards]})
    return cinemas
```

4. In `index()`, pass the movies map:

```python
            cinemas = _group_showings(payload.get("showings", []), payload.get("movies"))
```

5. Add the poster route inside `create_app` (next to `showings_ics`):

```python
    @app.route("/posters/<name>")
    def poster(name):
        return send_from_directory(Path(data_dir) / "posters", name, max_age=86400)
```

6. Template: replace the movie-card block with the filmrow structure:

```html
    {% for m in c.movies %}
    <div class="card">
      <div class="filmrow">
        {% if m.poster %}<img src="/posters/{{ m.poster }}" alt="" loading="lazy">{% endif %}
        <div class="filmtitle">
          <strong>{{ m.movie }}</strong>{% if m.badge %}<span class="badge">{{ m.badge }}</span>{% endif %}
          {% if m.meta_line %}<div class="filmmeta">{{ m.meta_line }}</div>{% endif %}
        </div>
      </div>
      {% for s in m.showings %}
      <a class="showing" href="{{ s.url }}"><span class="when">{{ s.date }} · {{ s.time }}</span>{% if s.detail %}<span class="detail">{{ s.detail }}</span>{% endif %}</a>
      {% endfor %}
    </div>
    {% endfor %}
```

7. CSS: add inside the `<style>` block (next to `.card strong`):

```css
  .filmrow{display:flex;gap:.8rem;align-items:flex-start}
  .filmrow img{width:58px;border-radius:4px;border:1px solid var(--edge);flex:0 0 auto}
  .filmtitle{min-width:0}
  .filmmeta{color:var(--dim);font-size:.8rem;margin-top:.15rem}
```

- [ ] **Step 4: Run the full suite**

Run: `.venv/bin/python -m pytest -q`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add app/web.py tests/test_web.py
git commit -m "Show poster and genre/runtime on web movie cards"
```

---

### Task 5: Telegram — genre/runtime suffix on movie title lines

**Files:**
- Modify: `app/notify.py` (`format_message`)
- Modify: `app/checker.py` (pass `movies=` at the notify call site)
- Test: `tests/test_notify.py`, `tests/test_checker.py`

**Interfaces:**
- Consumes: `movie_dicts` built in `run_check` (Task 3).
- Produces: `format_message(showings: list[Showing], movies: dict | None = None) -> str`. Title line becomes `<b>Title (OV)</b> — Genre1, Genre2, N Min`; suffix parts omitted when unknown; applied with or without the version badge.

- [ ] **Step 1: Write the failing tests**

Append to `tests/test_notify.py`:

```python
def test_format_message_appends_genre_and_runtime():
    showings = [
        make("Cineplexx Linz", "The Odyssey", 20, 19, 0, "OV", hall="Saal 6")
    ]
    movies = {
        "Cineplexx Linz|The Odyssey": {
            "runtime_min": 180,
            "genres": ["Abenteuer", "Historie"],
            "poster": None,
            "poster_file": None,
        }
    }
    msg = format_message(showings, movies=movies)
    assert "<b>The Odyssey (OV)</b> — Abenteuer, Historie, 180 Min" in msg


def test_format_message_meta_suffix_without_uniform_version():
    showings = [
        make("Cineplexx Linz", "F1", 20, 19, 0, "OV", hall="Saal 6", url="https://x/1"),
        make("Cineplexx Linz", "F1", 22, 18, 30, "OmU", hall="Saal 1", url="https://x/2"),
    ]
    movies = {
        "Cineplexx Linz|F1": {
            "runtime_min": 100,
            "genres": ["Drama"],
            "poster": None,
            "poster_file": None,
        }
    }
    msg = format_message(showings, movies=movies)
    assert "<b>F1</b> — Drama, 100 Min" in msg


def test_format_message_without_movies_is_unchanged():
    showings = [
        make("Cineplexx Linz", "The Odyssey", 20, 19, 0, "OV", hall="Saal 6")
    ]
    assert format_message(showings) == format_message(showings, movies={})


def test_format_message_escapes_meta():
    showings = [make("Cineplexx Linz", "X", 20, 19, 0, "OV")]
    movies = {
        "Cineplexx Linz|X": {
            "runtime_min": None,
            "genres": ["Dra<ma> & Co"],
            "poster": None,
            "poster_file": None,
        }
    }
    msg = format_message(showings, movies=movies)
    assert "Dra&lt;ma&gt; &amp; Co" in msg
```

Append to `tests/test_checker.py`:

```python
def test_new_showings_message_includes_meta(tmp_path):
    sent = []
    metas = {"Cineplexx Linz|The Odyssey": MovieMeta(180, ("Abenteuer",), None)}
    fetchers = {"cineplexx": FakeFetcher([make_showing()], metas=metas)}
    cfg = Config(telegram_token="T", telegram_chat_id="C", sources=("cineplexx",))
    run_check(None, tmp_path, cfg, NOW,
              notifier=lambda t, c, text: sent.append(text),
              fetcher_map=fetchers)
    assert "Abenteuer, 180 Min" in sent[0]
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `.venv/bin/python -m pytest tests/test_notify.py tests/test_checker.py -v`
Expected: FAIL — no suffix in messages; checker test fails on missing meta in text.

- [ ] **Step 3: Implement**

In `app/notify.py`, change the signature and the movie-title block of `format_message`:

```python
def format_message(showings: list[Showing], movies: dict | None = None) -> str:
    movies = movies or {}
    lines = ["🎬 <b>Neue OV-Vorstellungen in Linz</b>", ""]
    by_cinema: dict[str, list[Showing]] = defaultdict(list)
    for s in showings:
        by_cinema[s.cinema].append(s)
    for cinema in sorted(by_cinema):
        lines.append(f"<b>{escape(cinema)}</b>")
        by_movie: dict[str, list[Showing]] = defaultdict(list)
        for s in by_cinema[cinema]:
            by_movie[s.movie].append(s)
        # movie blocks ordered by their earliest showing
        for movie in sorted(by_movie, key=lambda m: min(x.start for x in by_movie[m])):
            group = sorted(by_movie[movie], key=lambda x: x.start)
            uniform_version = len({s.version for s in group}) == 1
            title = escape(movie)
            if uniform_version:
                title += f" ({escape(group[0].version)})"
            meta = movies.get(f"{cinema}|{movie}") or {}
            meta_parts = [escape(g) for g in meta.get("genres") or []]
            if meta.get("runtime_min"):
                meta_parts.append(f"{meta['runtime_min']} Min")
            if meta_parts:
                title += " — " + ", ".join(meta_parts)
            lines.append(f"<b>{title}</b>")
            for s in group:
                weekday = _WEEKDAYS[s.start.weekday()]
                parts = []
                if s.hall:
                    parts.append(escape(s.hall))
                parts.append(f"{weekday} {s.start:%d.%m}., {s.start:%H:%M}")
                if not uniform_version:
                    parts.append(escape(s.version))
                label = " · ".join(parts)
                lines.append(f'• <a href="{escape(s.url, quote=True)}">{label}</a>')
        lines.append("")
    return "\n".join(lines).strip()
```

In `app/checker.py`, change the notify call:

```python
    if new and can_notify:
        notifier(
            config.telegram_token,
            config.telegram_chat_id,
            notify.format_message(new, movies=movie_dicts),
        )
```

(`movie_dicts` is built above this block since Task 3.)

- [ ] **Step 4: Run the full suite**

Run: `.venv/bin/python -m pytest -q`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add app/notify.py app/checker.py tests/test_notify.py tests/test_checker.py
git commit -m "Add genre and runtime to Telegram movie titles"
```

---

### Task 6: ICS — real runtime for `DTEND`

**Files:**
- Modify: `app/ics.py` (`render_ics`, `_event`)
- Modify: `app/web.py` (pass `movies=` at the `/showings.ics` route)
- Test: `tests/test_ics.py`

**Interfaces:**
- Consumes: `showings.json` `"movies"` section (`runtime_min`).
- Produces: `render_ics(showings: list[dict], now: datetime | None = None, *, movies: dict | None = None) -> str`. `movies` is keyword-only; all existing callers pass `now=` by keyword and keep working. `DTEND = start + runtime_min` when known, else the fixed 2h fallback.

- [ ] **Step 1: Write the failing tests**

In `tests/test_ics.py`, extend `write_payload` with an optional movies map:

```python
def write_payload(data_dir, showings, movies=None):
    payload = {
        "generated_at": "2026-07-31T12:00:00+02:00",
        "sources": {"cineplexx": "ok"},
        "showings": showings,
    }
    if movies is not None:
        payload["movies"] = movies
    save_showings(data_dir, payload)
```

Append:

```python
ODYSSEY_MOVIES = {
    "Cineplexx Linz|The Odyssey": {
        "runtime_min": 121,
        "genres": ["Abenteuer"],
        "poster": None,
        "poster_file": None,
    }
}


def test_dtend_uses_runtime_when_known():
    body = render_ics([SHOWING], now=NOW, movies=ODYSSEY_MOVIES)
    # 19:00 at +02:00 = 17:00 UTC; +121 min = 19:01 UTC
    assert "DTSTART:20260802T170000Z" in body
    assert "DTEND:20260802T190100Z" in body


def test_dtend_falls_back_to_two_hours_for_unknown_movie():
    movies = {"Cineplexx Linz|Some Other Film": {"runtime_min": 300}}
    body = render_ics([SHOWING], now=NOW, movies=movies)
    assert "DTEND:20260802T190000Z" in body  # still the 2h fallback


def test_route_uses_runtime_from_payload(tmp_path):
    write_payload(tmp_path, [SHOWING], movies=ODYSSEY_MOVIES)
    body = create_app(tmp_path).test_client().get("/showings.ics").data.decode()
    assert "DTEND:20260802T190100Z" in body
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `.venv/bin/python -m pytest tests/test_ics.py -v`
Expected: FAIL — `TypeError: render_ics() got an unexpected keyword argument 'movies'`; route test still 2h.

- [ ] **Step 3: Implement**

In `app/ics.py`:

1. `_event` takes the duration:

```python
def _event(s: dict, stamp: str, duration: timedelta) -> list[str]:
    start = datetime.fromisoformat(s["start"])
    summary = f"{s['movie']} ({s['version']})"
    location = s["cinema"] + (f", {s['hall']}" if s.get("hall") else "")
    description = s["version"]
    if s.get("hall"):
        description += f", {s['hall']}"
    description += f" — {s['url']}"
    return [
        "BEGIN:VEVENT",
        f"UID:{_uid(s)}",
        f"DTSTAMP:{stamp}",
        f"DTSTART:{_fmt_utc(start)}",
        f"DTEND:{_fmt_utc(start + duration)}",
        f"SUMMARY:{_escape(summary)}",
        f"LOCATION:{_escape(location)}",
        f"DESCRIPTION:{_escape(description)}",
        f"URL:{s['url']}",
        "END:VEVENT",
    ]
```

2. `render_ics` resolves the duration per showing:

```python
def _duration(s: dict, movies: dict) -> timedelta:
    meta = movies.get(f"{s['cinema']}|{s['movie']}") or {}
    runtime = meta.get("runtime_min")
    if isinstance(runtime, int) and runtime > 0:
        return timedelta(minutes=runtime)
    return _EVENT_DURATION


def render_ics(
    showings: list[dict], now: datetime | None = None, *, movies: dict | None = None
) -> str:
    movies = movies or {}
    stamp = _fmt_utc(now or datetime.now(timezone.utc))
    lines = list(_CAL_HEADER)
    for s in showings:
        try:
            lines.extend(_event(s, stamp, _duration(s, movies)))
        except (KeyError, ValueError):
            continue  # skip malformed showing, don't fail the whole feed
    lines.append("END:VCALENDAR")
    folded = [fl for line in lines for fl in _fold(line)]
    return "\r\n".join(folded) + "\r\n"
```

In `app/web.py`, update the `/showings.ics` route:

```python
    @app.route("/showings.ics")
    def showings_ics():
        payload = state_mod.load_showings(data_dir) or {}
        body = ics_mod.render_ics(
            payload.get("showings", []), movies=payload.get("movies")
        )
        return Response(body, mimetype="text/calendar")
```

- [ ] **Step 4: Run the full suite**

Run: `.venv/bin/python -m pytest -q`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add app/ics.py app/web.py tests/test_ics.py
git commit -m "Use real movie runtime for ICS event durations"
```

---

### Task 7: Docs + full verification

**Files:**
- Modify: `AGENTS.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: everything above.

- [ ] **Step 1: Update `AGENTS.md`**

In the Layout section:

- Change the `app/fetchers.py` bullet to: `` `app/fetchers.py` — cinema program fetchers (cineplexx, megaplex); each returns `(showings, movie_metas)` ``
- Change the `app/checker.py` bullet to: `` `app/checker.py` — dedup/pruning + check orchestration (`Config`, `run_check`); caches poster images under `DATA_DIR/posters/` ``
- Change the `app/web.py` bullet to: `` `app/web.py` — read-only web UI (`create_app(data_dir)`); serves cached posters at `/posters/<name>` ``
- After the `app/state.py` bullet, add a line: `showings.json` carries a `"movies"` map (`"Cinema|Title"` → runtime/genres/poster) alongside the flat `"showings"` list.

- [ ] **Step 2: Update `README.md`**

Insert the following paragraph right after the intro paragraph (after the line
„...auf einer Webseite.“), keeping the README's German:

```markdown
Laufzeit, Genre und Filmplakat werden direkt von den Kinoseiten gelesen und auf
der Webseite, in den Telegram-Alerts und im Kalender-Feed (`/showings.ics`)
angezeigt. Plakate werden lokal unter `DATA_DIR/posters/` zwischengespeichert,
statt bei jedem Seitenaufruf das Kino-CDN zu laden.
```

- [ ] **Step 3: Run the full suite**

Run: `.venv/bin/python -m pytest -q`
Expected: PASS — entire suite.

- [ ] **Step 4: Smoke-test the web UI**

```bash
./serve.sh restart && sleep 1
curl -s http://localhost:8080/ | grep -c filmrow   # page renders with new markup
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8080/posters/nope.jpg   # 404, no crash
./serve.sh stop
```

Expected: page contains `filmrow` markup (seeded demo data has no `"movies"` — verifies the backward-compatible fallback renders fine), poster route 404s cleanly.

- [ ] **Step 5: Commit**

```bash
git add AGENTS.md README.md
git commit -m "Document movie metadata and poster cache"
```
