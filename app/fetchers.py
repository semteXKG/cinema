"""Fetch and parse showings from Cineplexx (JSON API) and Megaplex (HTML)."""

from __future__ import annotations

import re
import time
from datetime import date, datetime, timedelta
from zoneinfo import ZoneInfo

import requests
from bs4 import BeautifulSoup

from .models import Showing, cineplexx_session_version, megaplex_version

CINEPLEXX_BASE = "https://app.cineplexx.at"
CINEPLEXX_HEADERS = {
    "CINEPLEXX-Platform": "WEB",
    "client-key": "308330b1-52a5-4883-aee3-304240c22ea1",
    "User-Agent": (
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
        "(KHTML, like Gecko) Chrome/120.0 Safari/537.36"
    ),
}

CINEPLEXX_CINEMA_ID = "1014"
CINEPLEXX_CINEMA_NAME = "Cineplexx Linz"

MEGAPLEX_DAYS = 14
MEGAPLEX_HEADERS = {
    "User-Agent": (
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
        "(KHTML, like Gecko) Chrome/120.0 Safari/537.36"
    ),
}


class SourceError(Exception):
    """A cinema source returned something structurally unexpected."""


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
        try:
            resp = self._session.get(
                url, headers=headers, params=params, timeout=20
            )
            resp.raise_for_status()
            if self._delay_s:
                time.sleep(self._delay_s)
            return resp
        except requests.RequestException as e:
            raise SourceError(f"GET {url} failed: {e}") from e


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


MEGAPLEX_BASE = "https://www.megaplex.at"
MEGAPLEX_CINEMA_NAME = "Megaplex PlusCity"

_TZ = ZoneInfo("Europe/Vienna")
_OV_LINK_RE = re.compile(r"^/film/linz/[^/]+/ov$")
_DAY_RE = re.compile(r"(\d{2})\.(\d{2})\.(\d{4})")
_TIME_RE = re.compile(r"(\d{1,2}):(\d{2})")


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
