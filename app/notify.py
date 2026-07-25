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
        for s in sorted(by_cinema[cinema], key=lambda x: x.start):
            weekday = _WEEKDAYS[s.start.weekday()]
            hall = f" · {escape(s.hall)}" if s.hall else ""
            lines.append(
                f'• <a href="{escape(s.url, quote=True)}">{escape(s.movie)}</a> '
                f"({escape(s.version)}) — "
                f"{weekday} {s.start:%d.%m}., {s.start:%H:%M}{hall}"
            )
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
