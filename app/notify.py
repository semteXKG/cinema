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
