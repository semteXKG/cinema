# Web UI "Cinematic Marquee" Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restyle the OV-showings web page into a cinematic "theater marquee" look, purely in the inline template of `app/web.py`.

**Architecture:** Single-file change. The Flask route, `_group_showings`, and all other modules stay untouched; only the `_TEMPLATE` string (HTML + CSS) is replaced. One new test file pins the new visual identity.

**Tech Stack:** Python 3, Flask (`render_template_string`), Jinja2, pytest, pure CSS (no JS), one Google Fonts `<link>` (Limelight).

**Spec:** `docs/superpowers/specs/2026-07-25-web-marquee-redesign-design.md`

## Global Constraints

Byte-exact requirements from `tests/test_web.py` (must pass **unmodified**):

- Cinema headings render as plain `<h2>Cineplexx Linz</h2>` / `<h2>Megaplex PlusCity</h2>` — no attributes, no nested markup.
- Badge markup is exactly `<span class="badge">…</span>` (no extra classes); the string `class="badge"` must appear exactly once per rendered badge.
- Source footer uses `<span class="ok">` / `<span class="err">` (exact class names, no extras).
- Required strings: `Noch keine Daten`, `Aktuell keine OV-Vorstellungen`, dates like `Mo 20.07.`, times like `19:00`, `>OV<` / `>OmU<` for mixed versions, hall/version details (`Saal 7`, `Dolby Vision 2D`); the literal `OV - ` must never appear.
- A movie title appears exactly once in the page (never echoed in header/counts).
- Keep `<html lang="de">`, viewport meta, and `<meta http-equiv="refresh" content="900">`.
- No logic changes: `create_app`, `_group_showings`, `_short_version` untouched.

---

### Task 1: Restyle `_TEMPLATE` as a cinematic marquee (TDD)

**Files:**
- Create: `tests/test_web_marquee.py`
- Modify: `app/web.py:14-62` (replace the `_TEMPLATE` string only)

**Interfaces:**
- Consumes: existing `create_app(data_dir)` and `save_showings(data_dir, payload)` from `app.state` (signature: `save_showings(data_dir, payload: dict) -> None`).
- Produces: unchanged public interface — `create_app(data_dir) -> Flask`. Only rendered HTML changes.

- [ ] **Step 1: Write the failing tests**

Create `tests/test_web_marquee.py`:

```python
"""Tests for the cinematic-marquee visual identity of the web page."""

from datetime import datetime
from zoneinfo import ZoneInfo

from app.state import save_showings
from app.web import create_app

TZ = ZoneInfo("Europe/Vienna")


def write_payload(data_dir, showings):
    save_showings(data_dir, {
        "generated_at": datetime(2026, 7, 18, 12, 0, tzinfo=TZ).isoformat(),
        "sources": {"cineplexx": "ok", "megaplex": "ok"},
        "showings": showings,
    })


def one_showing():
    return [{
        "cinema": "Cineplexx Linz", "movie": "The Odyssey",
        "start": "2026-07-20T19:00:00+02:00", "version": "OV",
        "hall": "Saal 7", "url": "https://cineplexx.at/film/die-odyssee",
    }]


def test_marquee_header_and_font(tmp_path):
    write_payload(tmp_path, one_showing())
    html = create_app(tmp_path).test_client().get("/").data.decode()
    assert 'class="marquee"' in html
    assert "Originalversionen in Linz" in html
    assert html.count('class="bulbs"') == 2
    assert "family=Limelight" in html  # Google Fonts display face


def test_marquee_styling_hooks(tmp_path):
    write_payload(tmp_path, one_showing())
    html = create_app(tmp_path).test_client().get("/").data.decode()
    assert ".card::before" in html          # film-strip perforation
    assert "border-bottom:double" in html   # cinema heading rule
    assert "a.showing:hover" in html        # ticket-stub hover


def test_empty_states_styled(tmp_path):
    html = create_app(tmp_path).test_client().get("/").data.decode()
    assert '<p class="empty">Noch keine Daten' in html
    write_payload(tmp_path, [])
    html = create_app(tmp_path).test_client().get("/").data.decode()
    assert '<p class="empty">Aktuell keine OV-Vorstellungen' in html
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `.venv/bin/pytest tests/test_web_marquee.py -q`
Expected: 3 FAILURES (no `marquee`/`bulbs`/`empty` classes, no Limelight link, no new CSS hooks in the current template).

- [ ] **Step 3: Replace `_TEMPLATE` in `app/web.py`**

Replace the entire `_TEMPLATE = """..."""` assignment (lines 14–62) with:

```python
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
```

Nothing else in `app/web.py` changes.

- [ ] **Step 4: Run new tests to verify they pass**

Run: `.venv/bin/pytest tests/test_web_marquee.py -q`
Expected: 3 passed.

- [ ] **Step 5: Run the full suite (old tests must pass unmodified)**

Run: `.venv/bin/pytest -q`
Expected: all tests pass, including the untouched `tests/test_web.py`.

- [ ] **Step 6: Smoke-render against real data**

Run:

```bash
.venv/bin/python - <<'EOF'
from app.web import create_app
html = create_app("data").test_client().get("/").data.decode()
for needle in ['class="marquee"', "Originalversionen in Linz",
               "family=Limelight", "<h2>Cineplexx Linz</h2>",
               'class="badge"', "Mo ", ".card::before"]:
    assert needle in html, needle
print("smoke ok,", len(html), "bytes")
EOF
```

Expected: `smoke ok, … bytes` (uses the real `data/showings.json`; cinema
names may differ from the example — if `<h2>Cineplexx Linz</h2>` fails,
check `data/showings.json` for actual cinema names and adjust that one
needle).

- [ ] **Step 7: Commit**

```bash
git add app/web.py tests/test_web_marquee.py docs/superpowers/plans/2026-07-25-web-marquee-redesign.md
git commit -m "Restyle web UI as cinematic marquee"
```

---

## Self-Review Notes

- **Spec coverage:** header/tagline/bulbs → Step 3 `.marquee`/`.bulbs`; h2 double rule → Step 3 `h2{border-bottom:double}`; film-strip card → `.card::before`; ticket rows → `a.showing` dashed border + hover; badge ticket tag → `.badge`; empty states → `.empty`; footer → `.meta`/`.ok`/`.err`; Limelight font → `<link>`; no logic changes → only `_TEMPLATE` replaced. All covered.
- **Placeholder scan:** none — every code step contains complete code.
- **Type consistency:** `create_app(data_dir)` / `save_showings(data_dir, payload)` used identically in tests and app; no renamed symbols.
- **Constraint re-check against new template:** `<h2>{{ c.name }}</h2>` renders byte-exact; `<span class="badge">{{ m.badge }}</span>` byte-exact; `class="badge"` appears once in markup (CSS uses `.badge{`, which does not contain the string `class="badge"`); footer `ok`/`err` spans unchanged; `>OV<`/`>OmU<` come from `<span class="detail">OV</span>`; no `OV - ` literal; movie title only inside `<strong>`; `lang="de"`, viewport, refresh meta all present.
