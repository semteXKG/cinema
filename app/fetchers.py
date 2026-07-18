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
