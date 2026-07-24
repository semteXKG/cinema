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
 .badge{display:inline-block;background:#e61961;color:#fff;border-radius:4px;padding:0 .4rem;font-size:.75rem;margin-left:.5rem}
 a{color:#7fb3ff;text-decoration:none}
 a.showing{display:block;color:#eee;padding:.2rem .3rem;border-radius:4px}
 a.showing:hover{background:#2a2a3a}
 a.showing .when{color:#e6b34d;display:inline-block;min-width:8.5rem}
 a.showing .detail{color:#999;font-size:.85rem}
 .meta{color:#888;font-size:.8rem;margin-top:1.5rem}
 .ok{color:#6f6}.err{color:#f66}
</style>
</head>
<body>
<h1>🎬 OV-Vorstellungen in Linz</h1>
{% if cinemas is none %}
  <p>Noch keine Daten — der erste Check läuft gerade.</p>
{% elif not cinemas %}
  <p>Aktuell keine OV-Vorstellungen gefunden.</p>
{% else %}
  {% for c in cinemas %}
  <h2>{{ c.name }}</h2>
    {% for m in c.movies %}
    <div class="card">
      <strong>{{ m.movie }}</strong>{% if m.badge %}<span class="badge">{{ m.badge }}</span>{% endif %}
      {% for s in m.showings %}
      <a class="showing" href="{{ s.url }}"><span class="when">{{ s.date }} · {{ s.time }}</span>{% if s.detail %}<span class="detail">{{ s.detail }}</span>{% endif %}</a>
      {% endfor %}
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


def _short_version(version: str) -> str:
    """Strip the redundant leading 'OV'/'OV - ' prefix (page lists only OV)."""
    v = version.strip()
    if v == "OV":
        return ""
    if v.startswith("OV - "):
        return v[5:].strip()
    return v


def _group_showings(showings: list[dict]) -> list[dict]:
    """Group flat showing dicts into cinema -> movie -> showing rows."""
    parsed = sorted(
        ((datetime.fromisoformat(s["start"]), s) for s in showings),
        key=lambda x: x[0],
    )
    by_cinema: dict[str, dict[str, list]] = defaultdict(lambda: defaultdict(list))
    for start, s in parsed:
        by_cinema[s["cinema"]][s["movie"]].append((start, s))

    cinemas = []
    for cinema in sorted(by_cinema):
        movies = []
        for movie, entries in by_cinema[cinema].items():
            versions = {s["version"] for _, s in entries}
            badge = next(iter(versions)) if len(versions) == 1 else None
            rows = []
            for start, s in entries:
                parts = []
                if badge is None:
                    parts.append(_short_version(s["version"]) or s["version"])
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
            movies.append((entries[0][0], {"movie": movie, "badge": badge, "showings": rows}))
        movies.sort(key=lambda t: t[0])
        cinemas.append({"name": cinema, "movies": [m for _, m in movies]})
    return cinemas


def create_app(data_dir) -> Flask:
    app = Flask(__name__)

    @app.route("/healthz")
    def healthz():
        return "ok", 200

    @app.route("/")
    def index():
        payload = state_mod.load_showings(data_dir)
        cinemas = None
        generated_at = "–"
        sources: dict = {}
        if payload:
            generated_at = payload.get("generated_at", "–")
            sources = payload.get("sources") or {}
            cinemas = _group_showings(payload.get("showings", []))
        return render_template_string(
            _TEMPLATE, cinemas=cinemas, generated_at=generated_at, sources=sources
        )

    return app
