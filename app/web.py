"""Tiny read-only web UI for the current OV showings."""

from __future__ import annotations

from collections import defaultdict
from datetime import datetime
from pathlib import Path

from flask import Flask, Response, render_template_string, send_from_directory

from . import state as state_mod
from . import ics as ics_mod

_WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]

# Cinemas listed here render first, in this order; the rest alphabetically.
_CINEMA_ORDER = ("Megaplex PlusCity",)

_TEMPLATE = """<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="refresh" content="900">
<title>OV Cinema Linz</title>
<link rel="icon" type="image/svg+xml" href="/static/favicon.svg">
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
   max-width:860px;margin:0 auto;padding:1.5rem 1rem 2.5rem;
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
  .filmrow{display:flex;gap:.8rem;align-items:center}
  .filmrow img{width:58px;border-radius:4px;border:1px solid var(--edge);flex:0 0 auto}
  .filmtitle{min-width:0}
  .filmmeta{color:var(--dim);font-size:.8rem;margin-top:.15rem}
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
  .layout{display:flex;gap:1.2rem;align-items:flex-start}
  .layout main{flex:1;min-width:0}
  .layout main h2:first-child{margin-top:.2rem}
  .sidebar{flex:0 0 170px;position:sticky;top:1rem;
   display:flex;flex-direction:column;gap:.8rem}
  .sidebar .box{
   border:1px solid var(--edge);border-radius:8px;
   background:linear-gradient(180deg,#1a1410,#171109);
   padding:.9rem .8rem;
   display:flex;flex-direction:column;align-items:center;gap:.5rem;
   text-align:center;
  }
  .sidebar .box .icon{display:inline-flex;align-items:center;justify-content:center;
   width:28px;height:28px;flex:0 0 auto;font-size:1.4rem}
  .sidebar .box .icon svg{width:22px;height:22px;display:block}
  .sidebar .box .icon.tg{filter:drop-shadow(0 0 6px rgba(34,158,217,.4))}
  .sidebar .box .text{color:var(--text);font-size:.88rem}
  .sidebar .box .text .sub{color:var(--dim);font-size:.72rem;display:block;margin-top:.1rem}
  .sidebar .box a{width:100%;text-align:center;color:#221a0c;background:var(--gold);
   border-radius:4px;padding:.35rem .7rem;font-size:.75rem;font-weight:700;
   letter-spacing:.08em;box-shadow:0 0 8px rgba(232,179,77,.35)}
  .sidebar .box a:hover{background:var(--gold-bright)}
  @media (max-width:560px){
   .layout{flex-direction:column}
   .sidebar{position:static;flex-direction:column}
   .sidebar .box{flex-direction:row;text-align:left;padding:.7rem .75rem;gap:.4rem}
   .sidebar .box .icon{width:22px;height:22px;font-size:1.2rem}
   .sidebar .box .text{font-size:.82rem;flex:1}
   .sidebar .box a{width:auto;padding:.3rem .6rem;font-size:.7rem}
  }
  .meta{color:var(--faint);font-size:.8rem;margin-top:2rem;text-align:center}
 .ok{color:var(--ok)}.err{color:var(--err)}
</style>
</head>
<body>
<header class="marquee">
 <div class="bulbs"></div>
 <h1>🎬 OV Cinema Linz</h1>
 <p class="tagline">Original Versions in Linz</p>
 <div class="bulbs"></div>
</header>
<div class="layout">
<aside class="sidebar">
<div class="box">
  <span class="icon tg"><svg viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
    <circle cx="24" cy="24" r="24" fill="#229ED9"/>
    <path fill="#fff" d="M10.7 23.5l25-9.6c1.2-.4 2.2.3 1.8 2l-4.3 20c-.3 1.3-1 1.6-2 1l-6-4.4-2.9 2.8c-.3.3-.6.6-1.2.6l.4-6 10.6-9.6c.5-.4-.1-.6-.7-.2L17.2 22l-5.9-1.8c-1.3-.4-1.3-1.3.3-2z"/>
  </svg></span>
  <span class="text">Get notified about new OV showings on Telegram
    <span class="sub">Channel: @ov_linz — free, no spam, only new showings.</span>
  </span>
  <a href="https://t.me/ov_linz" target="_blank" rel="noopener">JOIN</a>
</div>
<div class="box">
  <span class="icon">📅</span>
  <span class="text">Add showings to your calendar
    <span class="sub">Subscribe in Google, Apple or Outlook Calendar.</span>
  </span>
  <a href="/showings.ics">SUBSCRIBE</a>
</div>
</aside>
<main>
{% if cinemas is none %}
  <p class="empty">No data yet — the first check is running.</p>
{% elif not cinemas %}
  <p class="empty">No OV showings found right now.</p>
{% else %}
  {% for c in cinemas %}
  <h2>{{ c.name }}</h2>
    {% for m in c.movies %}
    <div class="card">
      <div class="filmrow">
        {% if m.poster %}<img src="/posters/{{ m.poster }}" alt="" loading="lazy">{% endif %}
        <div class="filmtitle">
          <strong>{{ m.movie }}</strong>{% if m.badge %}<span class="badge">{{ m.badge }}</span>{% endif %}
          {% if m.meta_line %}<div class="filmmeta">{{ m.meta_line }}</div>{% endif %}
        </div>
      </div>
      {% for s in m.showings %}
      <a class="showing" href="{{ s.url }}"><span class="when">{{ s.date }} · {{ s.time }}</span>{% if s.detail %}<span class="detail">{{ s.detail }}</span>{% endif %}</a>
      {% endfor %}
    </div>
    {% endfor %}
  {% endfor %}
{% endif %}
</main>
</div>
<p class="meta">
  Last checked: {{ generated_at }} ·
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


def _cinema_key(name: str) -> tuple:
    preferred = _CINEMA_ORDER.index(name) if name in _CINEMA_ORDER else len(_CINEMA_ORDER)
    return (preferred, name)


def _meta_line(meta: dict) -> str:
    parts = []
    genres = [g for g in meta.get("genres") or [] if isinstance(g, str)]
    if genres:
        parts.append(", ".join(genres))
    if meta.get("runtime_min"):
        parts.append(f"{meta['runtime_min']} Min")
    return " · ".join(parts)


def _group_showings(showings: list[dict], movies: dict | None = None) -> list[dict]:
    """Group flat showing dicts into cinema -> movie -> showing rows."""
    metas = movies or {}
    parsed = sorted(
        ((datetime.fromisoformat(s["start"]), s) for s in showings),
        key=lambda x: x[0],
    )
    by_cinema: dict[str, dict[str, list]] = defaultdict(lambda: defaultdict(list))
    for start, s in parsed:
        by_cinema[s["cinema"]][s["movie"]].append((start, s))

    cinemas = []
    for cinema in sorted(by_cinema, key=_cinema_key):
        movie_cards = []
        for movie, entries in by_cinema[cinema].items():
            meta = metas.get(f"{cinema}|{movie}") or {}
            bases = {s["version"].split(" - ")[0].strip() for _, s in entries}
            badge = next(iter(bases)) if len(bases) == 1 else None
            rows = []
            for start, s in entries:
                parts = []
                variant = _short_version(s["version"])
                if badge is None:
                    parts.append(variant or s["version"])
                elif variant:
                    parts.append(variant)
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
            movie_cards.append(
                (
                    entries[0][0],
                    {
                        "movie": movie,
                        "badge": badge,
                        "showings": rows,
                        "poster": meta.get("poster_file"),
                        "meta_line": _meta_line(meta),
                    },
                )
            )
        movie_cards.sort(key=lambda t: t[0])
        cinemas.append({"name": cinema, "movies": [m for _, m in movie_cards]})
    return cinemas


def _format_generated_at(iso: str) -> str:
    """Render the stored ISO timestamp for the footer, without fractional seconds."""
    try:
        return datetime.fromisoformat(iso).strftime("%Y-%m-%d %H:%M")
    except ValueError:
        return iso


def create_app(data_dir) -> Flask:
    app = Flask(__name__)

    @app.route("/healthz")
    def healthz():
        return "ok", 200

    @app.route("/showings.ics")
    def showings_ics():
        payload = state_mod.load_showings(data_dir) or {}
        body = ics_mod.render_ics(
            payload.get("showings", []), movies=payload.get("movies")
        )
        return Response(body, mimetype="text/calendar")

    @app.route("/posters/<name>")
    def poster(name):
        return send_from_directory(Path(data_dir) / "posters", name, max_age=86400)

    @app.route("/")
    def index():
        payload = state_mod.load_showings(data_dir)
        cinemas = None
        generated_at = "–"
        sources: dict = {}
        if payload:
            generated_at = _format_generated_at(payload.get("generated_at", "–"))
            sources = payload.get("sources") or {}
            cinemas = _group_showings(payload.get("showings", []), payload.get("movies"))
        return render_template_string(
            _TEMPLATE, cinemas=cinemas, generated_at=generated_at, sources=sources
        )

    return app
