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
