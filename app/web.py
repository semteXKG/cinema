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
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Limelight&display=swap" rel="stylesheet">
<style>
 :root{
  --bg:#0f0c09;--panel:#1c1611;--edge:#3a2f22;
  --gold:#e8b34d;--gold-bright:#f5c56b;
  --text:#f3ead9;--dim:#a89a83;--faint:#7d7160;
  --ok:#8fc98f;--err:#e07a6a;
 }
 *{box-sizing:border-box}
 body{
  background:var(--bg);
  background-image:radial-gradient(ellipse at 50% -10%,#241b10 0%,var(--bg) 60%);
  color:var(--text);
  font-family:system-ui,-apple-system,sans-serif;
  max-width:760px;margin:0 auto;padding:1.5rem 1rem 2.5rem;
 }
 .marquee{
  border:2px solid var(--gold);border-radius:10px;
  background:#171209;
  box-shadow:0 0 24px rgba(232,179,77,.25),inset 0 0 30px rgba(232,179,77,.08);
  padding:.7rem 1rem .9rem;text-align:center;margin-bottom:1.8rem;
 }
 .marquee h1{
  font-family:'Limelight',system-ui,sans-serif;font-weight:400;
  color:var(--gold-bright);font-size:2rem;letter-spacing:.18em;
  margin:.4rem 0 .25rem;
  text-shadow:0 0 12px rgba(245,197,107,.55),0 0 34px rgba(232,179,77,.3);
 }
 .tagline{
  color:var(--dim);font-size:.75rem;letter-spacing:.35em;
  text-transform:uppercase;margin:0 0 .3rem;
 }
 .bulbs{
  height:10px;
  background-image:radial-gradient(circle,var(--gold-bright) 1.6px,rgba(232,179,77,.15) 2.6px,transparent 3px);
  background-size:22px 10px;background-position:center;background-repeat:repeat-x;
  filter:drop-shadow(0 0 4px rgba(245,197,107,.8));
 }
 h2{
  color:var(--gold);font-size:.95rem;letter-spacing:.22em;
  text-transform:uppercase;margin:1.8rem 0 .7rem;padding-bottom:.4rem;
  border-bottom:double 3px var(--edge);
 }
 .card{
  position:relative;
  background:var(--panel);border:1px solid var(--edge);border-radius:8px;
  padding:.7rem 1rem .8rem 2.1rem;margin:.6rem 0;
 }
 .card::before{
  content:"";position:absolute;left:.5rem;top:.6rem;bottom:.6rem;width:10px;
  border-radius:2px;background-color:#2a2117;
  background-image:radial-gradient(circle at 50% 50%,var(--bg) 1.7px,transparent 2.4px);
  background-size:10px 14px;
 }
 .card strong{
  font-family:'Limelight',system-ui,sans-serif;font-weight:400;
  font-size:1.15rem;letter-spacing:.06em;
 }
 .badge{
  display:inline-block;background:var(--gold);color:#221a0c;border-radius:3px;
  padding:.05rem .45rem;font-size:.7rem;font-weight:700;letter-spacing:.12em;
  margin-left:.6rem;vertical-align:.15em;
  box-shadow:0 0 8px rgba(232,179,77,.35);
 }
 a{color:var(--gold-bright);text-decoration:none}
 a.showing{
  display:flex;align-items:baseline;gap:.6rem;
  color:var(--text);
  padding:.35rem .55rem;margin-top:.35rem;
  border:1px dashed var(--edge);border-radius:5px;
  transition:transform .12s ease,box-shadow .12s ease,border-color .12s ease;
 }
 a.showing:hover{
  transform:translateY(-1px);
  background:#231c12;border-color:var(--gold);
  box-shadow:0 2px 14px rgba(232,179,77,.25);
 }
 a.showing .when{
  color:var(--gold-bright);display:inline-block;min-width:9.5rem;
  font-variant-numeric:tabular-nums;letter-spacing:.04em;
 }
 a.showing .detail{color:var(--dim);font-size:.85rem}
 .empty{
  text-align:center;color:var(--dim);
  border:1px dashed var(--edge);border-radius:8px;
  padding:2rem 1rem;margin:1.5rem 0;
 }
 .meta{color:var(--faint);font-size:.8rem;margin-top:2rem;text-align:center}
 .ok{color:var(--ok)}.err{color:var(--err)}
</style>
</head>
<body>
<header class="marquee">
 <div class="bulbs"></div>
 <h1>🎬 OV-Kino Linz</h1>
 <p class="tagline">Originalversionen in Linz</p>
 <div class="bulbs"></div>
</header>
{% if cinemas is none %}
  <p class="empty">Noch keine Daten — der erste Check läuft gerade.</p>
{% elif not cinemas %}
  <p class="empty">Aktuell keine OV-Vorstellungen gefunden.</p>
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
