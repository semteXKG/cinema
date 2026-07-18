"""Fetch and parse showings from Cineplexx (JSON API) and Megaplex (HTML)."""

from __future__ import annotations

import re
from datetime import date, datetime, timedelta
from zoneinfo import ZoneInfo

from bs4 import BeautifulSoup

from .models import Showing, cineplexx_session_version, megaplex_version

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
