"""Telegram notifications."""

from __future__ import annotations

from collections import defaultdict
from html import escape

import requests

from .models import Showing

_WEEKDAYS = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"]


def format_message(showings: list[Showing]) -> str:
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


def format_error(source: str, error: Exception) -> str:
    return (
        f"⚠️ OV-Watcher: Quelle „{escape(source)}“ scheint defekt: "
        f"{escape(str(error))}"
    )


_MAX_LEN = 4096  # Telegram sendMessage text limit


def _chunk_text(text: str, limit: int = _MAX_LEN) -> list[str]:
    """Split text into <=limit chunks on line boundaries (hard-wrap fallback)."""
    chunks: list[str] = []
    current = ""
    for line in text.split("\n"):
        while len(line) > limit:  # single overlong line: hard-wrap
            if current:
                chunks.append(current)
                current = ""
            chunks.append(line[:limit])
            line = line[limit:]
        candidate = f"{current}\n{line}" if current else line
        if len(candidate) <= limit:
            current = candidate
        else:
            chunks.append(current)
            current = line
    if current:
        chunks.append(current)
    return chunks


def send_telegram(token: str, chat_id: str, text: str, post=None) -> None:
    post = post or requests.post
    for chunk in _chunk_text(text):
        resp = post(
            f"https://api.telegram.org/bot{token}/sendMessage",
            json={
                "chat_id": chat_id,
                "text": chunk,
                "parse_mode": "HTML",
                "link_preview_options": {"is_disabled": True},
            },
            timeout=20,
        )
        resp.raise_for_status()
