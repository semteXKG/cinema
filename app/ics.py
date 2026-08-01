"""ICS calendar feed rendering (RFC 5545)."""

from __future__ import annotations

import hashlib
from datetime import datetime, timedelta, timezone

_EVENT_DURATION = timedelta(hours=2)

_CAL_HEADER = (
    "BEGIN:VCALENDAR",
    "VERSION:2.0",
    "PRODID:-//ov-kino-linz//EN",
    "CALSCALE:GREGORIAN",
    "METHOD:PUBLISH",
    "X-WR-CALNAME:OV Cinema Linz",
)


def _escape(text: str) -> str:
    return (
        text.replace("\\", "\\\\")
        .replace(";", "\\;")
        .replace(",", "\\,")
        .replace("\n", "\\n")
    )


def _fold(line: str) -> list[str]:
    """Fold a content line to <=75-octet chunks; continuations start with a space."""
    out: list[str] = []
    current, limit = "", 75
    for ch in line:
        width = len(ch.encode("utf-8"))
        if len(current.encode("utf-8")) + width > limit:
            out.append(current)
            current, limit = " " + ch, 74  # leading space counts toward 75
        else:
            current += ch
    out.append(current)
    return out


def _fmt_utc(dt: datetime) -> str:
    return dt.astimezone(timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def _uid(s: dict) -> str:
    key = f"{s['cinema']}|{s['movie']}|{s['start']}"
    return hashlib.sha1(key.encode("utf-8")).hexdigest() + "@ov-kino-linz"


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
