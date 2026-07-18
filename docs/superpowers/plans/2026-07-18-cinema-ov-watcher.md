# Cinema OV Watcher Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a watcher that detects new English OV/OmU showings at Cineplexx Linz and Megaplex PlusCity, sends Telegram alerts, and serves a web page of upcoming showings — deployable to a local k8s cluster.

**Architecture:** Single Python container: a checker (fetch → diff → notify → persist) run by an in-process scheduler thread, plus a Flask web app rendering the current showings. State in JSON files on a PVC-mounted dir. Spec: `docs/superpowers/specs/2026-07-18-cinema-ov-watcher-design.md`.

**Tech Stack:** Python 3.13, requests, BeautifulSoup4, Flask, pytest.

## Global Constraints

- Python ≥ 3.11 (uses `datetime.fromisoformat` with offsets, `zoneinfo`)
- Deps: `requests`, `beautifulsoup4`, `flask`; dev: `pytest`. No other deps.
- Cineplexx headers **verbatim**: `CINEPLEXX-Platform: WEB`, `client-key: 308330b1-52a5-4883-aee3-304240c22ea1`
- Cineplexx cinema ID: `1014` (Cineplexx Linz); display name `Cineplexx Linz`
- Megaplex base `https://www.megaplex.at`, cinema slug `linz`; display name `Megaplex PlusCity`
- Megaplex/browser User-Agent: `Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36`
- Timezone for parsed local times: `Europe/Vienna`
- 0.5 s politeness delay between HTTP requests
- Notification and web UI text: German
- TDD: failing test first, then minimal implementation, then commit
- Workdir for all commands: `/home/semtex/repo/cinema`

---

### Task 1: Project scaffold + fixtures

**Files:**
- Create: `requirements.txt`, `requirements-dev.txt`, `.gitignore`, `app/__init__.py`, `tests/__init__.py`, `tests/conftest.py`
- Create: `tests/fixtures/` (4 files copied from research captures)

**Interfaces:**
- Produces: `tests/conftest.py` exposes `FIXTURES_DIR: Path` and a `load_fixture(name: str) -> str` helper used by all later test tasks.

- [ ] **Step 1: Create project files**

```bash
mkdir -p app tests/fixtures
```

`requirements.txt`:
```
requests>=2.31
beautifulsoup4>=4.12
flask>=3.0
```

`requirements-dev.txt`:
```
-r requirements.txt
pytest>=8.0
```

`.gitignore`:
```
.venv/
__pycache__/
*.pyc
/data/
```

`app/__init__.py` and `tests/__init__.py`: empty files.

`tests/conftest.py`:
```python
from pathlib import Path

FIXTURES_DIR = Path(__file__).parent / "fixtures"


def load_fixture(name: str) -> str:
    return (FIXTURES_DIR / name).read_text(encoding="utf-8")
```

- [ ] **Step 2: Copy fixtures from research captures**

```bash
cp /tmp/opencode/linz-movies.json tests/fixtures/cineplexx_movies.json
cp /tmp/opencode/grp.json tests/fixtures/cineplexx_sessions_odyssey.json
cp /tmp/opencode/mega-ov.html tests/fixtures/megaplex_ov_program.html
cp /tmp/opencode/mega-film.html tests/fixtures/megaplex_film_ov.html
```

If any file is missing, re-capture:
```bash
curl -s "https://app.cineplexx.at/api/v1/cinemasweb/1014/movies?date=all" -H "CINEPLEXX-Platform: WEB" -H "client-key: 308330b1-52a5-4883-aee3-304240c22ea1" > tests/fixtures/cineplexx_movies.json
curl -s "https://app.cineplexx.at/api/v2/moviesweb/HO00016814/sessions?location=AUT" -H "CINEPLEXX-Platform: WEB" -H "client-key: 308330b1-52a5-4883-aee3-304240c22ea1" > tests/fixtures/cineplexx_sessions_odyssey.json
curl -s "https://www.megaplex.at/kinoprogramm/linz/2026-07-20/ov" -H "User-Agent: Mozilla/5.0" > tests/fixtures/megaplex_ov_program.html
curl -s "https://www.megaplex.at/film/linz/die-odyssee/ov" -H "User-Agent: Mozilla/5.0" > tests/fixtures/megaplex_film_ov.html
```

- [ ] **Step 3: Set up venv and verify pytest works**

```bash
python3 -m venv .venv && .venv/bin/pip install -q -r requirements-dev.txt
```

Write `tests/test_smoke.py`:
```python
def test_smoke():
    assert True
```

Run: `.venv/bin/pytest -q`
Expected: `1 passed`

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "Scaffold project with fixtures and test setup"
```

---

### Task 2: models.py — Showing dataclass + version matching

**Files:**
- Create: `app/models.py`
- Test: `tests/test_models.py`

**Interfaces:**
- Produces:
  - `Showing(cinema: str, movie: str, start: datetime, version: str, hall: str, url: str)` frozen dataclass with `.key -> str` (`"cinema|movie|start_isoformat"`)
  - `is_english_ov_label(label: str) -> bool`
  - `cineplexx_session_version(session: dict) -> str | None` — returns `"OV"`/`"OmU"`/`"OmdU"` or `None`
  - `megaplex_version(label: str) -> str | None` — returns the stripped label if it starts with `"OV"`, else `None`

- [ ] **Step 1: Write the failing tests**

`tests/test_models.py`:
```python
from datetime import datetime
from zoneinfo import ZoneInfo

from app.models import (
    Showing,
    cineplexx_session_version,
    is_english_ov_label,
    megaplex_version,
)

TZ = ZoneInfo("Europe/Vienna")


def make_showing():
    return Showing(
        cinema="Cineplexx Linz",
        movie="The Odyssey",
        start=datetime(2026, 7, 20, 19, 0, tzinfo=TZ),
        version="OV",
        hall="Saal 6",
        url="https://cineplexx.at/film/die-odyssee",
    )


def test_showing_key():
    s = make_showing()
    assert s.key == f"Cineplexx Linz|The Odyssey|{s.start.isoformat()}"


def test_is_english_ov_label():
    assert is_english_ov_label("OV (Englisch)")
    assert is_english_ov_label("OmU (Englisch)")
    assert is_english_ov_label("OV")
    assert is_english_ov_label("OmU")
    assert not is_english_ov_label("2D")
    assert not is_english_ov_label("IMAX")
    assert not is_english_ov_label("OV (Französisch)")
    assert not is_english_ov_label("")


def test_cineplexx_session_version_from_technologies():
    session = {"technologies": [["2D", "OV (Englisch)"], []], "conceptAttributesNames": ["OV"]}
    assert cineplexx_session_version(session) == "OV"


def test_cineplexx_session_version_omu():
    session = {"technologies": [["2D", "OmU (Englisch)"], []], "conceptAttributesNames": []}
    assert cineplexx_session_version(session) == "OmU"


def test_cineplexx_session_version_german_dub():
    session = {"technologies": [["2D"], []], "conceptAttributesNames": ["Wertvoll"]}
    assert cineplexx_session_version(session) is None


def test_cineplexx_session_version_non_english_ov():
    session = {"technologies": [["2D", "OV (Französisch)"], []], "conceptAttributesNames": []}
    assert cineplexx_session_version(session) is None


def test_megaplex_version():
    assert megaplex_version("OV - IMAX 2D") == "OV - IMAX 2D"
    assert megaplex_version("  OV - Dolby Vision 2D  ") == "OV - Dolby Vision 2D"
    assert megaplex_version("Dolby Atmos 2D") is None
    assert megaplex_version("4DX 2D") is None
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `.venv/bin/pytest tests/test_models.py -q`
Expected: FAIL (`ModuleNotFoundError: No module named 'app.models'`)

- [ ] **Step 3: Implement app/models.py**

```python
"""Core data model and OV/OmU version matching."""

from __future__ import annotations

import re
from dataclasses import dataclass
from datetime import datetime

_VERSION_RE = re.compile(r"\b(OV|OmU|OmdU)\b")
_LANG_RE = re.compile(r"\(([^)]*)\)")


@dataclass(frozen=True)
class Showing:
    cinema: str
    movie: str
    start: datetime  # timezone-aware
    version: str
    hall: str
    url: str

    @property
    def key(self) -> str:
        return f"{self.cinema}|{self.movie}|{self.start.isoformat()}"


def is_english_ov_label(label: str) -> bool:
    """True if a version label marks an English original version."""
    if not _VERSION_RE.search(label):
        return False
    lang = _LANG_RE.search(label)
    if lang and "englisch" not in lang.group(1).lower():
        return False
    return True


def cineplexx_session_version(session: dict) -> str | None:
    """Return 'OV'/'OmU'/'OmdU' for an English OV session, else None."""
    tech = [t for group in session.get("technologies", []) for t in group]
    for label in tech:
        m = _VERSION_RE.search(label)
        if m and is_english_ov_label(label):
            return m.group(1)
    for attr in session.get("conceptAttributesNames") or []:
        if attr in ("OV", "OmU", "OmdU"):
            return attr
    return None


def megaplex_version(label: str) -> str | None:
    """Megaplex tags original-language showings with a leading 'OV'."""
    label = " ".join(label.split())
    return label if label.startswith("OV") else None
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `.venv/bin/pytest tests/test_models.py -q`
Expected: `7 passed`

- [ ] **Step 5: Commit**

```bash
git add app/models.py tests/test_models.py
git commit -m "Add Showing model and OV version matching"
```

---

### Task 3: Cineplexx parser

**Files:**
- Create: `app/fetchers.py` (parser part only in this task; fetch wrappers come in Task 5)
- Test: `tests/test_fetchers_cineplexx.py`

**Interfaces:**
- Consumes: `Showing`, `cineplexx_session_version` from `app.models`
- Produces:
  - `CINEPLEXX_CINEMA_NAME = "Cineplexx Linz"`, `CINEPLEXX_CINEMA_ID = "1014"`
  - `parse_cineplexx_showings(movies: list[dict], sessions_by_movie: dict[str, list]) -> list[Showing]`

**Fixture ground truth** (verified 2026-07-18): `cineplexx_sessions_odyssey.json` (movie `HO00016814`, "The Odyssey") contains exactly **6** OV sessions at cinema `1014`, on 2026-07-20/21/22/23/24/26. The movies fixture lists 17 movies; sessions are only provided for `HO00016814`.

- [ ] **Step 1: Write the failing tests**

`tests/test_fetchers_cineplexx.py`:
```python
import json

from tests.conftest import load_fixture
from app.fetchers import parse_cineplexx_showings


def load():
    movies = json.loads(load_fixture("cineplexx_movies.json"))
    odyssey_sessions = json.loads(load_fixture("cineplexx_sessions_odyssey.json"))
    return movies, {"HO00016814": odyssey_sessions}


def test_finds_only_ov_sessions_at_linz():
    movies, sessions = load()
    showings = parse_cineplexx_showings(movies, sessions)
    assert len(showings) == 6
    assert all(s.version == "OV" for s in showings)
    assert all(s.cinema == "Cineplexx Linz" for s in showings)


def test_showing_fields():
    movies, sessions = load()
    showings = parse_cineplexx_showings(movies, sessions)
    s = showings[0]
    assert s.movie == "The Odyssey"  # leading '*' stripped
    assert s.url == "https://cineplexx.at/film/die-odyssee"
    assert s.hall  # non-empty hall
    assert s.start.tzinfo is not None  # timezone-aware
    assert {x.start.day for x in showings} == {20, 21, 22, 23, 24, 26}


def test_sessions_from_other_cinemas_ignored():
    movies, sessions = load()
    showings = parse_cineplexx_showings(movies, sessions)
    # fixture contains hundreds of sessions from other cinemas (Apollo, Innsbruck, ...)
    assert len(showings) == 6
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `.venv/bin/pytest tests/test_fetchers_cineplexx.py -q`
Expected: FAIL (`ModuleNotFoundError: No module named 'app.fetchers'`)

- [ ] **Step 3: Implement the parser in app/fetchers.py**

```python
"""Fetch and parse showings from Cineplexx (JSON API) and Megaplex (HTML)."""

from __future__ import annotations

from datetime import datetime

from .models import Showing, cineplexx_session_version

CINEPLEXX_CINEMA_ID = "1014"
CINEPLEXX_CINEMA_NAME = "Cineplexx Linz"


def parse_cineplexx_showings(
    movies: list[dict], sessions_by_movie: dict[str, list]
) -> list[Showing]:
    showings = []
    for movie in movies:
        title = (movie.get("title") or "").lstrip("*").strip()
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
    return showings
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `.venv/bin/pytest tests/test_fetchers_cineplexx.py -q`
Expected: `3 passed`

- [ ] **Step 5: Commit**

```bash
git add app/fetchers.py tests/test_fetchers_cineplexx.py
git commit -m "Add Cineplexx showings parser"
```

---

### Task 4: Megaplex parsers

**Files:**
- Modify: `app/fetchers.py` (add Megaplex parsing)
- Test: `tests/test_fetchers_megaplex.py`

**Interfaces:**
- Consumes: `Showing`, `megaplex_version` from `app.models`
- Produces:
  - `MEGAPLEX_BASE = "https://www.megaplex.at"`, `MEGAPLEX_CINEMA_NAME = "Megaplex PlusCity"`
  - `parse_megaplex_ov_links(html: str) -> list[str]` — unique absolute `/film/linz/{slug}/ov` URLs
  - `parse_megaplex_film_page(html: str, url: str, today: date) -> list[Showing]`
  - `SourceError(Exception)`

**Fixture ground truth** (verified 2026-07-18):
- `megaplex_ov_program.html` (program for 2026-07-20) → 3 unique OV film links: `/film/linz/die-odyssee/ov`, `/film/linz/insekten/ov`, `/film/linz/vaiana/ov`
- `megaplex_film_ov.html` (die-odyssee, fetched 2026-07-18) → 7 OV showings: 18.07 19:30 + 21:30, 19.07 19:30, 20.07 19:45, 21.07 20:30, 22.07 19:45, 23.07 19:45; first is `OV - Dolby Vision 2D` at `/ticket/57419/539128`; movie title `Die Odyssee`

- [ ] **Step 1: Write the failing tests**

`tests/test_fetchers_megaplex.py`:
```python
from datetime import date

from tests.conftest import load_fixture
from app.fetchers import (
    MEGAPLEX_BASE,
    parse_megaplex_film_page,
    parse_megaplex_ov_links,
)

TODAY = date(2026, 7, 18)


def test_parse_ov_links_unique_and_absolute():
    html = load_fixture("megaplex_ov_program.html")
    links = parse_megaplex_ov_links(html)
    assert links == [
        f"{MEGAPLEX_BASE}/film/linz/die-odyssee/ov",
        f"{MEGAPLEX_BASE}/film/linz/insekten/ov",
        f"{MEGAPLEX_BASE}/film/linz/vaiana/ov",
    ]


def test_parse_film_page_showings():
    html = load_fixture("megaplex_film_ov.html")
    url = f"{MEGAPLEX_BASE}/film/linz/die-odyssee/ov"
    showings = parse_megaplex_film_page(html, url, TODAY)
    assert len(showings) == 7
    assert all(s.cinema == "Megaplex PlusCity" for s in showings)
    assert all(s.movie == "Die Odyssee" for s in showings)
    assert all(s.version.startswith("OV") for s in showings)


def test_parse_film_page_dates_and_links():
    html = load_fixture("megaplex_film_ov.html")
    url = f"{MEGAPLEX_BASE}/film/linz/die-odyssee/ov"
    showings = parse_megaplex_film_page(html, url, TODAY)
    first = showings[0]
    assert first.start.day == 18
    assert (first.start.hour, first.start.minute) == (19, 30)
    assert first.version == "OV - Dolby Vision 2D"
    assert first.url == f"{MEGAPLEX_BASE}/ticket/57419/539128"
    assert first.start.tzinfo is not None
    days = sorted(s.start.day for s in showings)
    assert days == [18, 18, 19, 20, 21, 22, 23]
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `.venv/bin/pytest tests/test_fetchers_megaplex.py -q`
Expected: FAIL (`ImportError: cannot import name 'MEGAPLEX_BASE'`)

- [ ] **Step 3: Implement Megaplex parsing in app/fetchers.py**

Add to `app/fetchers.py` (keep existing Cineplexx code):

```python
import re
from datetime import date, timedelta
from zoneinfo import ZoneInfo

from bs4 import BeautifulSoup

from .models import megaplex_version

MEGAPLEX_BASE = "https://www.megaplex.at"
MEGAPLEX_CINEMA_NAME = "Megaplex PlusCity"

_TZ = ZoneInfo("Europe/Vienna")
_OV_LINK_RE = re.compile(r"^/film/linz/[^/]+/ov$")
_DAY_RE = re.compile(r"(\d{2})\.(\d{2})\.(\d{4})")
_TIME_RE = re.compile(r"(\d{1,2}):(\d{2})")


class SourceError(Exception):
    """A cinema source returned something structurally unexpected."""


def parse_megaplex_ov_links(html: str) -> list[str]:
    soup = BeautifulSoup(html, "html.parser")
    links: list[str] = []
    for a in soup.find_all("a", href=_OV_LINK_RE):
        url = MEGAPLEX_BASE + a["href"]
        if url not in links:
            links.append(url)
    return links


def _parse_day(label: str, today: date) -> date | None:
    label = " ".join(label.split())
    if label == "Heute":
        return today
    if label == "Morgen":
        return today + timedelta(days=1)
    m = _DAY_RE.search(label)
    if m:
        return date(int(m.group(3)), int(m.group(2)), int(m.group(1)))
    return None


def parse_megaplex_film_page(html: str, url: str, today: date) -> list[Showing]:
    soup = BeautifulSoup(html, "html.parser")
    if "Kinoprogramm" not in soup.get_text():
        raise SourceError(f"unexpected Megaplex film page: {url}")
    h1 = soup.find("h1")
    title = ""
    if h1:
        raw = h1.get_text(" ", strip=True)
        title = re.split(r"\s*\(Pluscity\)|\s+-\s+OV", raw)[0].strip()
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
    return showings
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `.venv/bin/pytest tests/test_fetchers_megaplex.py -q`
Expected: `3 passed`

- [ ] **Step 5: Commit**

```bash
git add app/fetchers.py tests/test_fetchers_megaplex.py
git commit -m "Add Megaplex program and film page parsers"
```

---

### Task 5: HTTP fetchers with retry + sanity checks

**Files:**
- Modify: `app/fetchers.py` (add `HttpClient`, `fetch_cineplexx`, `fetch_megaplex`)
- Test: `tests/test_fetchers_http.py`

**Interfaces:**
- Consumes: parsers from Tasks 3–4, `SourceError`
- Produces:
  - `class HttpClient(delay_s: float = 0.0)` with `get_json(url, headers=None, params=None)` and `get_text(url, headers=None)`; one retry on network error, raises `SourceError` on failure/non-JSON
  - `fetch_cineplexx(http) -> list[Showing]`
  - `fetch_megaplex(http, today: date) -> list[Showing]`
  - Tests use a fake `http` object with the same two methods (duck-typed)

- [ ] **Step 1: Write the failing tests**

`tests/test_fetchers_http.py`:
```python
import json
from datetime import date

import pytest

from tests.conftest import load_fixture
from app.fetchers import (
    SourceError,
    fetch_cineplexx,
    fetch_megaplex,
)

TODAY = date(2026, 7, 18)


class FakeHttp:
    """Serves fixtures for known URLs; records requested URLs."""

    def __init__(self):
        self.json_routes = {}
        self.text_routes = {}
        self.requested = []

    def get_json(self, url, headers=None, params=None):
        self.requested.append(url)
        for key, value in self.json_routes.items():
            if key in url:
                return value
        raise SourceError(f"unexpected URL {url}")

    def get_text(self, url, headers=None):
        self.requested.append(url)
        for key, value in self.text_routes.items():
            if key in url:
                return value
        raise SourceError(f"unexpected URL {url}")


def make_cineplexx_http():
    http = FakeHttp()
    http.json_routes["/cinemasweb/1014/movies"] = json.loads(
        load_fixture("cineplexx_movies.json")
    )
    sessions = json.loads(load_fixture("cineplexx_sessions_odyssey.json"))
    http.json_routes["/sessions"] = sessions
    return http


def test_fetch_cineplexx_returns_ov_showings():
    showings = fetch_cineplexx(make_cineplexx_http())
    assert len(showings) == 6
    assert all(s.cinema == "Cineplexx Linz" for s in showings)


def test_fetch_cineplexx_empty_movie_list_is_source_error():
    http = FakeHttp()
    http.json_routes["/cinemasweb/1014/movies"] = []
    with pytest.raises(SourceError):
        fetch_cineplexx(http)


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
    showings = fetch_megaplex(http, TODAY)
    assert len(showings) == 7
    assert all(s.cinema == "Megaplex PlusCity" for s in showings)
    # 14 program pages (one per day) + film pages
    program_calls = [u for u in http.requested if "/kinoprogramm/linz/" in u]
    assert len(program_calls) == 14


def test_fetch_megaplex_broken_page_is_source_error():
    http = FakeHttp()
    http.text_routes["/kinoprogramm/linz/"] = "<html><body>404</body></html>"
    with pytest.raises(SourceError):
        fetch_megaplex(http, TODAY)
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `.venv/bin/pytest tests/test_fetchers_http.py -q`
Expected: FAIL (`ImportError: cannot import name 'fetch_cineplexx'`)

- [ ] **Step 3: Implement HTTP layer in app/fetchers.py**

Add to `app/fetchers.py`:

```python
import time

import requests

CINEPLEXX_BASE = "https://app.cineplexx.at"
CINEPLEXX_HEADERS = {
    "CINEPLEXX-Platform": "WEB",
    "client-key": "308330b1-52a5-4883-aee3-304240c22ea1",
    "User-Agent": (
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
        "(KHTML, like Gecko) Chrome/120.0 Safari/537.36"
    ),
}

MEGAPLEX_DAYS = 14
MEGAPLEX_HEADERS = {
    "User-Agent": (
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
        "(KHTML, like Gecko) Chrome/120.0 Safari/537.36"
    ),
}


class HttpClient:
    def __init__(self, delay_s: float = 0.0):
        self._session = requests.Session()
        self._delay_s = delay_s

    def get_json(self, url, headers=None, params=None):
        resp = self._get(url, headers=headers, params=params)
        try:
            return resp.json()
        except ValueError as e:
            raise SourceError(f"no JSON from {url}") from e

    def get_text(self, url, headers=None):
        return self._get(url, headers=headers, params=None).text

    def _get(self, url, headers, params):
        last_error: Exception | None = None
        for _ in range(2):
            try:
                resp = self._session.get(
                    url, headers=headers, params=params, timeout=20
                )
                resp.raise_for_status()
                if self._delay_s:
                    time.sleep(self._delay_s)
                return resp
            except requests.RequestException as e:
                last_error = e
        raise SourceError(f"GET {url} failed: {last_error}") from last_error


def fetch_cineplexx(http) -> list[Showing]:
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
    return parse_cineplexx_showings(movies, sessions_by_movie)


def fetch_megaplex(http, today: date) -> list[Showing]:
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
    for url in links:
        html = http.get_text(url, headers=MEGAPLEX_HEADERS)
        showings.extend(parse_megaplex_film_page(html, url, today))
    return showings
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `.venv/bin/pytest tests/test_fetchers_http.py -q`
Expected: `4 passed`

- [ ] **Step 5: Commit**

```bash
git add app/fetchers.py tests/test_fetchers_http.py
git commit -m "Add HTTP fetchers with retry and sanity checks"
```

---

### Task 6: state.py — persistence, pruning

**Files:**
- Create: `app/state.py`
- Test: `tests/test_state.py`

**Interfaces:**
- Consumes: `Showing` from `app.models`
- Produces:
  - `load_state(data_dir) -> dict` (keys `seen: dict`, `error_pings: dict`; corrupt file → renamed to `.bak`, default returned)
  - `save_state(data_dir, state: dict) -> None`
  - `save_showings(data_dir, payload: dict) -> None`
  - `load_showings(data_dir) -> dict | None`
  - `showing_to_dict(s: Showing) -> dict`
  - `prune_state(state: dict, now: datetime) -> None` (drops `seen` keys whose start < now − 6 h)

- [ ] **Step 1: Write the failing tests**

`tests/test_state.py`:
```python
import json
from datetime import datetime, timedelta
from zoneinfo import ZoneInfo

from app.models import Showing
from app.state import (
    load_showings,
    load_state,
    prune_state,
    save_showings,
    save_state,
    showing_to_dict,
)

TZ = ZoneInfo("Europe/Vienna")
NOW = datetime(2026, 7, 18, 12, 0, tzinfo=TZ)


def make_showing(day, hour=19):
    return Showing(
        cinema="Cineplexx Linz",
        movie="The Odyssey",
        start=datetime(2026, 7, day, hour, 0, tzinfo=TZ),
        version="OV",
        hall="Saal 6",
        url="https://cineplexx.at/film/die-odyssee",
    )


def test_load_state_missing_returns_default(tmp_path):
    state = load_state(tmp_path)
    assert state == {"seen": {}, "error_pings": {}}


def test_state_roundtrip(tmp_path):
    state = {"seen": {"k": "2026-07-18T00:00:00+02:00"}, "error_pings": {}}
    save_state(tmp_path, state)
    assert load_state(tmp_path) == state


def test_load_state_corrupt_backs_up(tmp_path):
    (tmp_path / "state.json").write_text("{not json", encoding="utf-8")
    state = load_state(tmp_path)
    assert state == {"seen": {}, "error_pings": {}}
    assert (tmp_path / "state.json.bak").exists()


def test_showings_roundtrip(tmp_path):
    payload = {"generated_at": NOW.isoformat(), "sources": {}, "showings": []}
    save_showings(tmp_path, payload)
    assert load_showings(tmp_path) == payload


def test_load_showings_missing(tmp_path):
    assert load_showings(tmp_path) is None


def test_showing_to_dict_serializable():
    d = showing_to_dict(make_showing(20))
    assert json.dumps(d)
    assert d["start"] == "2026-07-20T19:00:00+02:00"


def test_prune_state_drops_past_keys():
    old = make_showing(17).key   # >6h in the past relative to NOW
    future = make_showing(20).key
    state = {"seen": {old: NOW.isoformat(), future: NOW.isoformat()}, "error_pings": {}}
    prune_state(state, NOW)
    assert old not in state["seen"]
    assert future in state["seen"]
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `.venv/bin/pytest tests/test_state.py -q`
Expected: FAIL (`ModuleNotFoundError: No module named 'app.state'`)

- [ ] **Step 3: Implement app/state.py**

```python
"""Persistent state: dedup keys, error-ping rate limits, showings.json."""

from __future__ import annotations

import json
from datetime import datetime, timedelta
from pathlib import Path

from .models import Showing

_PRUNE_GRACE = timedelta(hours=6)


def _load_json(path: Path, default):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return default
    except (json.JSONDecodeError, OSError):
        path.rename(path.with_suffix(path.suffix + ".bak"))
        return default


def _save_json(path: Path, payload) -> None:
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(payload, ensure_ascii=False, indent=1), encoding="utf-8")
    tmp.replace(path)


def load_state(data_dir) -> dict:
    state = _load_json(Path(data_dir) / "state.json", {})
    if not isinstance(state, dict):
        state = {}
    state.setdefault("seen", {})
    state.setdefault("error_pings", {})
    return state


def save_state(data_dir, state: dict) -> None:
    _save_json(Path(data_dir) / "state.json", state)


def save_showings(data_dir, payload: dict) -> None:
    _save_json(Path(data_dir) / "showings.json", payload)


def load_showings(data_dir) -> dict | None:
    return _load_json(Path(data_dir) / "showings.json", None)


def showing_to_dict(s: Showing) -> dict:
    return {
        "cinema": s.cinema,
        "movie": s.movie,
        "start": s.start.isoformat(),
        "version": s.version,
        "hall": s.hall,
        "url": s.url,
    }


def prune_state(state: dict, now: datetime) -> None:
    cutoff = now - _PRUNE_GRACE
    state["seen"] = {
        key: first_seen
        for key, first_seen in state["seen"].items()
        if datetime.fromisoformat(key.rsplit("|", 1)[1]) >= cutoff
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `.venv/bin/pytest tests/test_state.py -q`
Expected: `7 passed`

- [ ] **Step 5: Commit**

```bash
git add app/state.py tests/test_state.py
git commit -m "Add JSON state persistence with pruning"
```

---

### Task 7: notify.py — Telegram

**Files:**
- Create: `app/notify.py`
- Test: `tests/test_notify.py`

**Interfaces:**
- Consumes: `Showing` from `app.models`
- Produces:
  - `format_message(showings: list[Showing]) -> str`
  - `format_error(source: str, error: Exception) -> str`
  - `send_telegram(token: str, chat_id: str, text: str, post=None) -> None` (`post` injectable, default `requests.post`)

- [ ] **Step 1: Write the failing tests**

`tests/test_notify.py`:
```python
from datetime import datetime
from zoneinfo import ZoneInfo

from app.models import Showing
from app.notify import format_error, format_message, send_telegram

TZ = ZoneInfo("Europe/Vienna")


def make(cinema, movie, day, hour, minute, version, hall="", url="https://x"):
    return Showing(cinema, movie, datetime(2026, 7, day, hour, minute, tzinfo=TZ), version, hall, url)


def test_format_message_groups_by_cinema_and_formats_german():
    showings = [
        make("Megaplex PlusCity", "Die Odyssee", 20, 19, 45, "OV - IMAX 2D",
             url="https://www.megaplex.at/ticket/57419/539128"),
        make("Cineplexx Linz", "The Odyssey", 20, 19, 0, "OV", hall="Saal 6",
             url="https://cineplexx.at/film/die-odyssee"),
    ]
    msg = format_message(showings)
    lines = msg.split("\n")
    assert lines[0] == "🎬 Neue OV-Vorstellungen in Linz!"
    # cinemas sorted alphabetically: Cineplexx before Megaplex
    assert lines.index("Cineplexx Linz") < lines.index("Megaplex PlusCity")
    # 2026-07-20 is a Monday -> "Mo 20.07."
    assert "• The Odyssey (OV) — Mo 20.07., 19:00, Saal 6" in msg
    assert "• Die Odyssee (OV - IMAX 2D) — Mo 20.07., 19:45" in msg
    assert "https://cineplexx.at/film/die-odyssee" in msg
    assert "https://www.megaplex.at/ticket/57419/539128" in msg


def test_format_error():
    msg = format_error("Megaplex", ValueError("boom"))
    assert "Megaplex" in msg
    assert "boom" in msg


def test_send_telegram_posts_payload():
    calls = []

    class Resp:
        def raise_for_status(self):
            pass

    def fake_post(url, json, timeout):
        calls.append((url, json, timeout))
        return Resp()

    send_telegram("TOKEN", "123", "hello", post=fake_post)
    assert calls == [
        ("https://api.telegram.org/botTOKEN/sendMessage",
         {"chat_id": "123", "text": "hello"}, 20)
    ]
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `.venv/bin/pytest tests/test_notify.py -q`
Expected: FAIL (`ModuleNotFoundError: No module named 'app.notify'`)

- [ ] **Step 3: Implement app/notify.py**

```python
"""Telegram notifications."""

from __future__ import annotations

from collections import defaultdict

import requests

from .models import Showing

_WEEKDAYS = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"]


def format_message(showings: list[Showing]) -> str:
    lines = ["🎬 Neue OV-Vorstellungen in Linz!", ""]
    by_cinema: dict[str, list[Showing]] = defaultdict(list)
    for s in showings:
        by_cinema[s.cinema].append(s)
    for cinema in sorted(by_cinema):
        lines.append(cinema)
        for s in sorted(by_cinema[cinema], key=lambda x: x.start):
            weekday = _WEEKDAYS[s.start.weekday()]
            hall = f", {s.hall}" if s.hall else ""
            lines.append(
                f"• {s.movie} ({s.version}) — "
                f"{weekday} {s.start:%d.%m}., {s.start:%H:%M}{hall}"
            )
            lines.append(s.url)
        lines.append("")
    return "\n".join(lines).strip()


def format_error(source: str, error: Exception) -> str:
    return f'⚠️ OV-Watcher: Quelle „{source}“ scheint defekt: {error}'


def send_telegram(token: str, chat_id: str, text: str, post=None) -> None:
    post = post or requests.post
    resp = post(
        f"https://api.telegram.org/bot{token}/sendMessage",
        json={"chat_id": chat_id, "text": text},
        timeout=20,
    )
    resp.raise_for_status()
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `.venv/bin/pytest tests/test_notify.py -q`
Expected: `3 passed`

- [ ] **Step 5: Commit**

```bash
git add app/notify.py tests/test_notify.py
git commit -m "Add Telegram notifications"
```

---

### Task 8: checker.py — orchestration

**Files:**
- Create: `app/checker.py`
- Test: `tests/test_checker.py`

**Interfaces:**
- Consumes: `fetchers` (fetch functions, `SourceError`), `state`, `notify`
- Produces:
  - `Config(telegram_token: str | None, telegram_chat_id: str | None, sources: tuple[str, ...] = ("cineplexx", "megaplex"))`
  - `CheckResult(new_showings: int, total_showings: int, sources: dict[str, str])`
  - `run_check(http, data_dir, config: Config, now: datetime, notifier=None) -> CheckResult` (`notifier` injectable callable `(token, chat_id, text) -> None`, defaults to `notify.send_telegram`)

**Behavior (from spec):**
- Fetch enabled sources; per-source `SourceError` → health `"error"` + error ping (rate-limited: 1/day/source via `state["error_pings"]`)
- Only upcoming showings (`start >= now`) are kept
- New (unseen) showings → one Telegram message; every upcoming showing's key recorded in `seen`
- Prune past keys; always write `state.json` + `showings.json` (with `generated_at`, `sources` health, sorted showings)
- No Telegram configured → no sends, everything else still runs

- [ ] **Step 1: Write the failing tests**

`tests/test_checker.py`:
```python
from datetime import datetime
from zoneinfo import ZoneInfo

from app.checker import Config, run_check
from app.models import Showing
from app.state import load_showings, load_state

TZ = ZoneInfo("Europe/Vienna")
NOW = datetime(2026, 7, 18, 12, 0, tzinfo=TZ)


def make_showing(cinema="Cineplexx Linz", movie="The Odyssey", day=20):
    return Showing(cinema, movie, datetime(2026, 7, day, 19, 0, tzinfo=TZ),
                   "OV", "Saal 6", "https://x")


class FakeFetcher:
    def __init__(self, showings=None, error=None):
        self.showings = showings or []
        self.error = error

    def __call__(self, http, today=None):
        if self.error:
            raise self.error
        return self.showings


def test_new_showings_trigger_one_message(tmp_path):
    sent = []
    fetchers = {"cineplexx": FakeFetcher([make_showing()])}
    cfg = Config(telegram_token="T", telegram_chat_id="C", sources=("cineplexx",))
    result = run_check(None, tmp_path, cfg, NOW,
                       notifier=lambda t, c, text: sent.append(text),
                       fetcher_map=fetchers)
    assert result.new_showings == 1
    assert result.total_showings == 1
    assert len(sent) == 1
    assert "The Odyssey" in sent[0]

    # second run: same showing -> no new message
    result = run_check(None, tmp_path, cfg, NOW,
                       notifier=lambda t, c, text: sent.append(text),
                       fetcher_map=fetchers)
    assert result.new_showings == 0
    assert len(sent) == 1


def test_past_showings_are_dropped(tmp_path):
    sent = []
    past = make_showing(day=17)
    fetchers = {"cineplexx": FakeFetcher([past])}
    cfg = Config(telegram_token="T", telegram_chat_id="C", sources=("cineplexx",))
    result = run_check(None, tmp_path, cfg, NOW,
                       notifier=lambda t, c, text: sent.append(text),
                       fetcher_map=fetchers)
    assert result.total_showings == 0
    assert sent == []
    assert load_showings(tmp_path)["showings"] == []


def test_showings_json_written_with_health(tmp_path):
    fetchers = {"cineplexx": FakeFetcher([make_showing()])}
    cfg = Config(telegram_token=None, telegram_chat_id=None, sources=("cineplexx",))
    run_check(None, tmp_path, cfg, NOW, fetcher_map=fetchers)
    payload = load_showings(tmp_path)
    assert payload["sources"] == {"cineplexx": "ok"}
    assert payload["generated_at"] == NOW.isoformat()
    assert payload["showings"][0]["movie"] == "The Odyssey"


def test_source_error_sends_rate_limited_ping(tmp_path):
    from app.fetchers import SourceError

    sent = []
    err = SourceError("kaputt")
    fetchers = {"megaplex": FakeFetcher(error=err)}
    cfg = Config(telegram_token="T", telegram_chat_id="C", sources=("megaplex",))
    notifier = lambda t, c, text: sent.append(text)
    result = run_check(None, tmp_path, cfg, NOW, notifier=notifier, fetcher_map=fetchers)
    assert result.sources == {"megaplex": "error"}
    assert len(sent) == 1 and "kaputt" in sent[0]

    # same day: no second ping
    run_check(None, tmp_path, cfg, NOW, notifier=notifier, fetcher_map=fetchers)
    assert len(sent) == 1


def test_no_telegram_configured_still_writes_state(tmp_path):
    fetchers = {"cineplexx": FakeFetcher([make_showing()])}
    cfg = Config(telegram_token=None, telegram_chat_id=None, sources=("cineplexx",))
    run_check(None, tmp_path, cfg, NOW, fetcher_map=fetchers)
    assert make_showing().key in load_state(tmp_path)["seen"]
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `.venv/bin/pytest tests/test_checker.py -q`
Expected: FAIL (`ModuleNotFoundError: No module named 'app.checker'`)

- [ ] **Step 3: Implement app/checker.py**

```python
"""One check run: fetch -> diff -> notify -> persist."""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path

from . import fetchers, notify
from . import state as state_mod
from .models import Showing


@dataclass
class Config:
    telegram_token: str | None
    telegram_chat_id: str | None
    sources: tuple[str, ...] = ("cineplexx", "megaplex")


@dataclass
class CheckResult:
    new_showings: int = 0
    total_showings: int = 0
    sources: dict[str, str] = field(default_factory=dict)


def default_fetcher_map():
    return {
        "cineplexx": lambda http, today: fetchers.fetch_cineplexx(http),
        "megaplex": lambda http, today: fetchers.fetch_megaplex(http, today),
    }


def run_check(
    http,
    data_dir,
    config: Config,
    now: datetime,
    notifier=None,
    fetcher_map=None,
) -> CheckResult:
    notifier = notifier or notify.send_telegram
    fetcher_map = fetcher_map or default_fetcher_map()
    data_dir = Path(data_dir)
    data_dir.mkdir(parents=True, exist_ok=True)
    state = state_mod.load_state(data_dir)

    can_notify = bool(config.telegram_token and config.telegram_chat_id)

    all_showings: list[Showing] = []
    health: dict[str, str] = {}
    for source in config.sources:
        try:
            all_showings.extend(fetcher_map[source](http, now.date()))
            health[source] = "ok"
        except fetchers.SourceError as e:
            health[source] = "error"
            already_pinged = state["error_pings"].get(source) == now.date().isoformat()
            if can_notify and not already_pinged:
                notifier(
                    config.telegram_token,
                    config.telegram_chat_id,
                    notify.format_error(source, e),
                )
                state["error_pings"][source] = now.date().isoformat()

    upcoming = [s for s in all_showings if s.start >= now]
    new = [s for s in upcoming if s.key not in state["seen"]]
    if new and can_notify:
        notifier(
            config.telegram_token,
            config.telegram_chat_id,
            notify.format_message(new),
        )
    for s in upcoming:
        state["seen"].setdefault(s.key, now.isoformat())
    state_mod.prune_state(state, now)
    state_mod.save_state(data_dir, state)
    state_mod.save_showings(
        data_dir,
        {
            "generated_at": now.isoformat(),
            "sources": health,
            "showings": [
                state_mod.showing_to_dict(s)
                for s in sorted(upcoming, key=lambda x: (x.start, x.cinema))
            ],
        },
    )
    return CheckResult(len(new), len(upcoming), health)
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `.venv/bin/pytest tests/test_checker.py -q`
Expected: `5 passed`

- [ ] **Step 5: Commit**

```bash
git add app/checker.py tests/test_checker.py
git commit -m "Add check-run orchestration with dedup and health"
```

---

### Task 9: web.py — Flask web page

**Files:**
- Create: `app/web.py`
- Test: `tests/test_web.py`

**Interfaces:**
- Consumes: `state.load_showings`
- Produces: `create_app(data_dir) -> Flask` with routes `/` (HTML page) and `/healthz` (plain `ok`)

- [ ] **Step 1: Write the failing tests**

`tests/test_web.py`:
```python
from datetime import datetime
from zoneinfo import ZoneInfo

from app.state import save_showings
from app.web import create_app

TZ = ZoneInfo("Europe/Vienna")


def write_payload(data_dir, showings):
    save_showings(data_dir, {
        "generated_at": datetime(2026, 7, 18, 12, 0, tzinfo=TZ).isoformat(),
        "sources": {"cineplexx": "ok", "megaplex": "error"},
        "showings": showings,
    })


def test_healthz(tmp_path):
    client = create_app(tmp_path).test_client()
    resp = client.get("/healthz")
    assert resp.status_code == 200
    assert resp.data == b"ok"


def test_index_renders_showings_grouped_by_day(tmp_path):
    write_payload(tmp_path, [
        {"cinema": "Cineplexx Linz", "movie": "The Odyssey",
         "start": "2026-07-20T19:00:00+02:00", "version": "OV",
         "hall": "Saal 6", "url": "https://cineplexx.at/film/die-odyssee"},
        {"cinema": "Megaplex PlusCity", "movie": "Die Odyssee",
         "start": "2026-07-20T19:45:00+02:00", "version": "OV - IMAX 2D",
         "hall": "", "url": "https://www.megaplex.at/ticket/57419/539128"},
    ])
    client = create_app(tmp_path).test_client()
    html = client.get("/").data.decode()
    assert "The Odyssey" in html and "Die Odyssee" in html
    assert "OV - IMAX 2D" in html
    assert "Saal 6" in html
    assert "https://www.megaplex.at/ticket/57419/539128" in html
    assert "Mo 20.07.2026" in html  # day group header (Monday)
    assert 'class="err"' in html    # megaplex health shown as error


def test_index_without_data(tmp_path):
    client = create_app(tmp_path).test_client()
    html = client.get("/").data.decode()
    assert "Noch keine Daten" in html
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `.venv/bin/pytest tests/test_web.py -q`
Expected: FAIL (`ModuleNotFoundError: No module named 'app.web'`)

- [ ] **Step 3: Implement app/web.py**

```python
"""Tiny read-only web UI for the current OV showings."""

from __future__ import annotations

from collections import defaultdict
from datetime import datetime

from flask import Flask, render_template_string

from . import state as state_mod

_WEEKDAYS = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"]

_TEMPLATE = """<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="refresh" content="900">
<title>OV-Kino Linz</title>
<style>
 body{background:#14141c;color:#eee;font-family:system-ui,sans-serif;max-width:760px;margin:0 auto;padding:1rem}
 h1{color:#e6b34d;font-size:1.4rem}
 h2{color:#ba9765;font-size:1.05rem;margin:1.2rem 0 .4rem}
 .card{background:#1e1e2a;border:1px solid #333;border-radius:8px;padding:.6rem .9rem;margin:.4rem 0}
 .badge{display:inline-block;background:#e61961;color:#fff;border-radius:4px;padding:0 .4rem;font-size:.75rem;margin-right:.5rem}
 a{color:#7fb3ff;text-decoration:none}
 .meta{color:#888;font-size:.8rem;margin-top:1.5rem}
 .ok{color:#6f6}.err{color:#f66}
</style>
</head>
<body>
<h1>🎬 OV-Vorstellungen in Linz</h1>
{% if days is none %}
  <p>Noch keine Daten — der erste Check läuft gerade.</p>
{% elif not days %}
  <p>Aktuell keine OV-Vorstellungen gefunden.</p>
{% else %}
  {% for day, items in days %}
  <h2>{{ day }}</h2>
    {% for s in items %}
    <div class="card">
      <span class="badge">{{ s.version }}</span>
      <strong>{{ s.movie }}</strong> — {{ s.time }}{% if s.hall %}, {{ s.hall }}{% endif %} · {{ s.cinema }}<br>
      <a href="{{ s.url }}">Tickets →</a>
    </div>
    {% endfor %}
  {% endfor %}
{% endif %}
<p class="meta">
  Zuletzt geprüft: {{ generated_at }} ·
  Cineplexx: <span class="{{ 'ok' if sources.get('cineplexx') == 'ok' else 'err' }}">{{ sources.get('cineplexx', '–') }}</span> ·
  Megaplex: <span class="{{ 'ok' if sources.get('megaplex') == 'ok' else 'err' }}">{{ sources.get('megaplex', '–') }}</span>
</p>
</body>
</html>
"""


def create_app(data_dir) -> Flask:
    app = Flask(__name__)

    @app.route("/healthz")
    def healthz():
        return "ok", 200

    @app.route("/")
    def index():
        payload = state_mod.load_showings(data_dir)
        days = None
        generated_at = "–"
        sources: dict = {}
        if payload:
            generated_at = payload.get("generated_at", "–")
            sources = payload.get("sources") or {}
            parsed = []
            for s in payload.get("showings", []):
                parsed.append((datetime.fromisoformat(s["start"]), s))
            parsed.sort(key=lambda x: x[0])
            grouped: dict[str, list] = defaultdict(list)
            order: list[str] = []
            for start, s in parsed:
                label = f"{_WEEKDAYS[start.weekday()]} {start:%d.%m.%Y}"
                if label not in grouped:
                    order.append(label)
                grouped[label].append(
                    {
                        "time": f"{start:%H:%M}",
                        "movie": s["movie"],
                        "version": s["version"],
                        "hall": s.get("hall") or "",
                        "cinema": s["cinema"],
                        "url": s["url"],
                    }
                )
            days = [(label, grouped[label]) for label in order]
        return render_template_string(
            _TEMPLATE, days=days, generated_at=generated_at, sources=sources
        )

    return app
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `.venv/bin/pytest tests/test_web.py -q`
Expected: `3 passed`

- [ ] **Step 5: Commit**

```bash
git add app/web.py tests/test_web.py
git commit -m "Add read-only web page for upcoming OV showings"
```

---

### Task 10: main.py + Dockerfile + README

**Files:**
- Create: `app/main.py`, `Dockerfile`, `README.md`
- Test: `tests/test_main.py`

**Interfaces:**
- Consumes: `checker.run_check`, `checker.Config`, `fetchers.HttpClient`, `web.create_app`
- Produces:
  - `scheduler_loop(stop: threading.Event, interval_s: float, run: Callable[[], None]) -> None` — runs `run()` immediately, then every `interval_s`, swallows+logs exceptions, exits when `stop` is set
  - `main() -> None` — env config: `DATA_DIR` (default `/data`), `CHECK_INTERVAL_HOURS` (default `3`), `PORT` (default `8080`), `TELEGRAM_BOT_TOKEN`, `TELEGRAM_CHAT_ID`, `SOURCES` (default `cineplexx,megaplex`)

- [ ] **Step 1: Write the failing test**

`tests/test_main.py`:
```python
import threading

from app.main import scheduler_loop


def test_scheduler_loop_runs_until_stopped():
    calls = []

    def run():
        calls.append(1)
        if len(calls) >= 3:
            stop.set()

    stop = threading.Event()
    scheduler_loop(stop, 3600, run)  # huge interval; stop ends it
    assert len(calls) >= 3


def test_scheduler_loop_swallows_exceptions():
    calls = []

    def run():
        calls.append(1)
        if len(calls) >= 2:
            stop.set()
        raise RuntimeError("boom")

    stop = threading.Event()
    scheduler_loop(stop, 3600, run)
    assert len(calls) >= 2
```

- [ ] **Step 2: Run test to verify it fails**

Run: `.venv/bin/pytest tests/test_main.py -q`
Expected: FAIL (`ModuleNotFoundError: No module named 'app.main'`)

- [ ] **Step 3: Implement app/main.py**

```python
"""Entrypoint: scheduler thread + web server."""

from __future__ import annotations

import logging
import os
import threading
from datetime import datetime
from zoneinfo import ZoneInfo

from .checker import Config, run_check
from .fetchers import HttpClient
from .web import create_app

TZ = ZoneInfo("Europe/Vienna")
log = logging.getLogger("ov-watcher")


def scheduler_loop(stop: threading.Event, interval_s: float, run) -> None:
    while not stop.is_set():
        try:
            run()
        except Exception:
            log.exception("check run failed")
        stop.wait(interval_s)


def main() -> None:
    logging.basicConfig(
        level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s"
    )
    data_dir = os.environ.get("DATA_DIR", "/data")
    interval_s = float(os.environ.get("CHECK_INTERVAL_HOURS", "3")) * 3600
    port = int(os.environ.get("PORT", "8080"))
    config = Config(
        telegram_token=os.environ.get("TELEGRAM_BOT_TOKEN"),
        telegram_chat_id=os.environ.get("TELEGRAM_CHAT_ID"),
        sources=tuple(
            s.strip()
            for s in os.environ.get("SOURCES", "cineplexx,megaplex").split(",")
            if s.strip()
        ),
    )
    http = HttpClient(delay_s=0.5)
    stop = threading.Event()
    thread = threading.Thread(
        target=scheduler_loop,
        args=(stop, interval_s, lambda: run_check(http, data_dir, config, datetime.now(TZ))),
        daemon=True,
    )
    thread.start()
    log.info("starting web server on port %s", port)
    try:
        create_app(data_dir).run(host="0.0.0.0", port=port)
    finally:
        stop.set()


if __name__ == "__main__":
    main()
```

- [ ] **Step 4: Run test to verify it passes**

Run: `.venv/bin/pytest tests/test_main.py -q`
Expected: `2 passed`

- [ ] **Step 5: Write Dockerfile and README.md**

`Dockerfile`:
```dockerfile
FROM python:3.13-slim
WORKDIR /app
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt \
    && useradd -r -u 10001 ov \
    && mkdir -p /data \
    && chown ov /data
COPY app/ app/
USER ov
ENV DATA_DIR=/data PORT=8080
EXPOSE 8080
CMD ["python", "-m", "app.main"]
```

`README.md`:
```markdown
# Cinema OV Watcher

Findet neue OV/OmU-Vorstellungen (englische Originalfassungen) im
Cineplexx Linz und Hollywood Megaplex PlusCity, schickt Telegram-Alerts
und zeigt alle kommenden OV-Vorstellungen auf einer Webseite.

## Lokal laufen lassen

```bash
python3 -m venv .venv
.venv/bin/pip install -r requirements-dev.txt
DATA_DIR=./data \
TELEGRAM_BOT_TOKEN=... TELEGRAM_CHAT_ID=... \
.venv/bin/python -m app.main
# Webseite: http://localhost:8080
```

Telegram-Bot: bei @BotFather anlegen, Token notieren; Chat-ID z. B. über
@userinfobot.

## Tests

```bash
.venv/bin/pytest -q
```

## Docker

```bash
docker build -t cinema-ov-watcher:latest .
docker run --rm -p 8080:8080 \
  -e TELEGRAM_BOT_TOKEN=... -e TELEGRAM_CHAT_ID=... \
  -v ov-data:/data cinema-ov-watcher:latest
```

## Kubernetes

```bash
kubectl apply -f k8s/pvc.yaml -f k8s/secret.yaml -f k8s/configmap.yaml \
  -f k8s/deployment.yaml -f k8s/service.yaml
```

`k8s/secret.yaml` aus `k8s/secret.example.yaml` erzeugen und die echten
Werte eintragen (nicht committen).
```

- [ ] **Step 6: Run full test suite**

Run: `.venv/bin/pytest -q`
Expected: all tests pass (25)

- [ ] **Step 7: Commit**

```bash
git add app/main.py Dockerfile README.md tests/test_main.py
git commit -m "Add entrypoint, Dockerfile and README"
```

---

### Task 11: k8s manifests

**Files:**
- Create: `k8s/deployment.yaml`, `k8s/service.yaml`, `k8s/pvc.yaml`, `k8s/configmap.yaml`, `k8s/secret.example.yaml`

**Interfaces:**
- Consumes: container image `cinema-ov-watcher:latest` (Task 10 Dockerfile), port 8080, `/healthz`, `/data` mount, env vars from `main.py`

- [ ] **Step 1: Write manifests**

`k8s/pvc.yaml`:
```yaml
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: ov-watcher-data
spec:
  accessModes: ["ReadWriteOnce"]
  resources:
    requests:
      storage: 10Mi
```

`k8s/configmap.yaml`:
```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: ov-watcher-config
data:
  CHECK_INTERVAL_HOURS: "3"
  SOURCES: "cineplexx,megaplex"
```

`k8s/secret.example.yaml`:
```yaml
apiVersion: v1
kind: Secret
metadata:
  name: ov-watcher-secret
type: Opaque
stringData:
  TELEGRAM_BOT_TOKEN: "123456:ABC-replace-me"
  TELEGRAM_CHAT_ID: "12345678"
```

`k8s/deployment.yaml`:
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: ov-watcher
  labels:
    app: ov-watcher
spec:
  replicas: 1
  selector:
    matchLabels:
      app: ov-watcher
  strategy:
    type: Recreate
  template:
    metadata:
      labels:
        app: ov-watcher
    spec:
      containers:
        - name: ov-watcher
          image: cinema-ov-watcher:latest
          imagePullPolicy: IfNotPresent
          ports:
            - containerPort: 8080
          envFrom:
            - configMapRef:
                name: ov-watcher-config
            - secretRef:
                name: ov-watcher-secret
          volumeMounts:
            - name: data
              mountPath: /data
          livenessProbe:
            httpGet:
              path: /healthz
              port: 8080
            initialDelaySeconds: 10
            periodSeconds: 30
          readinessProbe:
            httpGet:
              path: /healthz
              port: 8080
            initialDelaySeconds: 5
            periodSeconds: 10
          resources:
            requests:
              cpu: 50m
              memory: 64Mi
            limits:
              cpu: 500m
              memory: 256Mi
      volumes:
        - name: data
          persistentVolumeClaim:
            claimName: ov-watcher-data
```

`k8s/service.yaml`:
```yaml
apiVersion: v1
kind: Service
metadata:
  name: ov-watcher
spec:
  type: ClusterIP
  selector:
    app: ov-watcher
  ports:
    - port: 8080
      targetPort: 8080
```

- [ ] **Step 2: Validate manifests (if kubectl is available)**

Run: `kubectl apply --dry-run=client -f k8s/`
Expected: each resource prints `... configured (dry run)`. If `kubectl` is not installed, skip and note it.

- [ ] **Step 3: Run full test suite + commit**

Run: `.venv/bin/pytest -q`
Expected: all tests pass

```bash
git add k8s/
git commit -m "Add k8s manifests"
```

---

### Task 12: Live smoke test (manual verification)

- [ ] **Step 1: Run the app locally against real sources**

```bash
DATA_DIR=./data .venv/bin/python -m app.main &
sleep 90   # first check takes ~30-60 s (HTTP politeness delay)
curl -s http://localhost:8080/ | grep -i -E "Odyssee|OV" | head -5
curl -s http://localhost:8080/healthz
kill %1
```

Expected: web page lists at least the *Die Odyssee* OV showings at both cinemas; `healthz` returns `ok`; `data/showings.json` and `data/state.json` exist. No Telegram message is sent (no token configured).

- [ ] **Step 2: Commit any fixes discovered**

Only if something failed above; fix forward with a test reproducing the issue first.
