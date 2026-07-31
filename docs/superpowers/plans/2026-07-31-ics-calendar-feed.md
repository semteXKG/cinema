# ICS Calendar Feed Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Serve a subscribable ICS feed of all upcoming OV/OmU showings at `/showings.ics` and advertise it in the web page sidebar.

**Architecture:** New pure module `app/ics.py` renders RFC 5545 text from the showing dicts in `showings.json`; `app/web.py` adds a read-only route returning it as `text/calendar`; the sidebar gains a subscribe link. No new dependencies, no state changes.

**Tech Stack:** Python 3.13, Flask, pytest. Stdlib only (`hashlib`, `datetime`).

## Global Constraints

- No new dependencies (requirements.txt stays flask+requests).
- Fixed 2h event duration (no end times are scraped).
- Event times rendered in UTC (`YYYYMMDDTHHMMSSZ`), no `VTIMEZONE` block.
- UID = `sha1(f"{cinema}|{movie}|{start}")` + `@ov-kino-linz` (stable across checks).
- CRLF line endings; lines folded to ≤75 octets (continuations start with a space).
- Escaping per RFC 5545: `\` → `\\`, `;` → `\;`, `,` → `\,`, newline → `\n`.
- Missing/empty payload → valid empty `VCALENDAR`, HTTP 200.
- Malformed `start` in a showing → skip that showing, don't fail the feed.
- UI copy in English.

---

### Task 1: `app/ics.py` — ICS renderer

**Files:**
- Create: `app/ics.py`
- Test: `tests/test_ics.py`

**Interfaces:**
- Consumes: showing dicts as stored in `showings.json` — keys `cinema: str`, `movie: str`, `start: str` (tz-aware ISO 8601), `version: str`, `hall: str`, `url: str`.
- Produces: `render_ics(showings: list[dict], now: datetime | None = None) -> str` — full `VCALENDAR` text (CRLF endings, folded lines). `now` injects the `DTSTAMP` value for deterministic tests; route callers omit it.

- [ ] **Step 1: Write the failing tests**

```python
"""Tests for the ICS calendar feed renderer."""

from datetime import datetime, timezone

from app.ics import render_ics

NOW = datetime(2026, 7, 31, 12, 0, tzinfo=timezone.utc)

SHOWING = {
    "cinema": "Cineplexx Linz",
    "movie": "The Odyssey",
    "start": "2026-08-02T19:00:00+02:00",
    "version": "OV",
    "hall": "Saal 7",
    "url": "https://cineplexx.at/f/x",
}


def _lines(body: str) -> list[str]:
    return body.split("\r\n")


def test_calendar_skeleton():
    body = render_ics([], now=NOW)
    assert body.startswith("BEGIN:VCALENDAR\r\n")
    assert body.endswith("END:VCALENDAR\r\n")
    assert "VERSION:2.0" in body
    assert "X-WR-CALNAME:OV-Kino Linz" in body
    assert "BEGIN:VEVENT" not in body


def test_event_times_are_utc_and_two_hours_apart():
    body = render_ics([SHOWING], now=NOW)
    # 19:00 at +02:00 = 17:00 UTC; DTEND two hours later
    assert "DTSTART:20260802T170000Z" in body
    assert "DTEND:20260802T190000Z" in body
    assert "DTSTAMP:20260731T120000Z" in body


def test_summary_location_description_url():
    body = render_ics([SHOWING], now=NOW)
    assert "SUMMARY:The Odyssey (OV)" in body
    assert "LOCATION:Cineplexx Linz\\, Saal 7" in body
    assert "URL:https://cineplexx.at/f/x" in body
    assert "DESCRIPTION:" in body


def test_uid_is_stable():
    a = render_ics([SHOWING], now=datetime(2026, 1, 1, tzinfo=timezone.utc))
    b = render_ics([SHOWING], now=datetime(2026, 1, 2, tzinfo=timezone.utc))
    uid_a = next(l for l in _lines(a) if l.startswith("UID:"))
    uid_b = next(l for l in _lines(b) if l.startswith("UID:"))
    assert uid_a == uid_b
    assert uid_a.endswith("@ov-kino-linz")


def test_text_escaping():
    s = {**SHOWING, "movie": "Foo, Bar; Baz"}
    body = render_ics([s], now=NOW)
    assert "SUMMARY:Foo\\, Bar\\; Baz (OV)" in body


def test_long_lines_folded_to_75_octets():
    s = {**SHOWING, "movie": "X" * 100}
    body = render_ics([s], now=NOW)
    for line in _lines(body):
        assert len(line.encode("utf-8")) <= 75


def test_malformed_start_is_skipped():
    bad = {**SHOWING, "start": "not-a-date"}
    body = render_ics([bad, SHOWING], now=NOW)
    assert body.count("BEGIN:VEVENT") == 1
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `.venv/bin/python -m pytest tests/test_ics.py -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'app.ics'`

- [ ] **Step 3: Write the implementation**

Create `app/ics.py`:

```python
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
    "X-WR-CALNAME:OV-Kino Linz",
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


def _event(s: dict, stamp: str) -> list[str]:
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
        f"DTEND:{_fmt_utc(start + _EVENT_DURATION)}",
        f"SUMMARY:{_escape(summary)}",
        f"LOCATION:{_escape(location)}",
        f"DESCRIPTION:{_escape(description)}",
        f"URL:{s['url']}",
        "END:VEVENT",
    ]


def render_ics(showings: list[dict], now: datetime | None = None) -> str:
    stamp = _fmt_utc(now or datetime.now(timezone.utc))
    lines = list(_CAL_HEADER)
    for s in showings:
        try:
            lines.extend(_event(s, stamp))
        except (KeyError, ValueError):
            continue  # skip malformed showing, don't fail the whole feed
    lines.append("END:VCALENDAR")
    folded = [fl for line in lines for fl in _fold(line)]
    return "\r\n".join(folded) + "\r\n"
```

Note: the f-string `f"{s['movie']} ({s['version']})"` reuses double quotes around single-quoted keys — fine on this project's Python (3.13, see `Dockerfile`).

- [ ] **Step 4: Run tests to verify they pass**

Run: `.venv/bin/python -m pytest tests/test_ics.py -v`
Expected: 7 passed

- [ ] **Step 5: Commit**

```bash
git add app/ics.py tests/test_ics.py
git commit -m "Add ICS calendar feed renderer"
```

---

### Task 2: `/showings.ics` route

**Files:**
- Modify: `app/web.py` (imports + new route, inside `create_app`)
- Test: `tests/test_ics.py` (append route tests)
- Modify: `AGENTS.md` (layout list)

**Interfaces:**
- Consumes: `render_ics(showings, now=None)` from Task 1; `state_mod.load_showings(data_dir)` already used by `/`.
- Produces: `GET /showings.ics` → 200, body = `render_ics(payload["showings"])`, content type `text/calendar`.

- [ ] **Step 1: Write the failing tests**

Append to `tests/test_ics.py`:

```python
from app.state import save_showings
from app.web import create_app


def write_payload(data_dir, showings):
    save_showings(data_dir, {
        "generated_at": "2026-07-31T12:00:00+02:00",
        "sources": {"cineplexx": "ok"},
        "showings": showings,
    })


def test_route_content_type_and_one_event_per_showing(tmp_path):
    second = {**SHOWING, "movie": "Film B", "start": "2026-08-03T20:00:00+02:00"}
    write_payload(tmp_path, [SHOWING, second])
    resp = create_app(tmp_path).test_client().get("/showings.ics")
    assert resp.status_code == 200
    assert resp.content_type.startswith("text/calendar")
    body = resp.data.decode()
    assert body.count("BEGIN:VEVENT") == 2
    assert "SUMMARY:The Odyssey (OV)" in body
    assert "SUMMARY:Film B (OV)" in body


def test_route_without_payload_returns_empty_calendar(tmp_path):
    resp = create_app(tmp_path).test_client().get("/showings.ics")
    assert resp.status_code == 200
    body = resp.data.decode()
    assert "BEGIN:VCALENDAR" in body
    assert "BEGIN:VEVENT" not in body
```

(`SHOWING` is already defined at the top of `tests/test_ics.py` from Task 1.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `.venv/bin/python -m pytest tests/test_ics.py -k route -v`
Expected: FAIL — 404 on `/showings.ics`

- [ ] **Step 3: Add the route**

In `app/web.py`, change the import line `from flask import Flask, render_template_string` to:

```python
from flask import Flask, Response, render_template_string
```

Add below the existing state import:

```python
from . import ics as ics_mod
```

Add this route inside `create_app`, directly after the `healthz` route:

```python
    @app.route("/showings.ics")
    def showings_ics():
        payload = state_mod.load_showings(data_dir) or {}
        body = ics_mod.render_ics(payload.get("showings", []))
        return Response(body, mimetype="text/calendar")
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `.venv/bin/python -m pytest -q`
Expected: all tests pass (52 existing + 9 new)

- [ ] **Step 5: Update AGENTS.md layout list**

In `AGENTS.md`, add to the Layout bullet list after the `app/web.py` line:

```markdown
- `app/ics.py` — ICS calendar feed renderer (`render_ics`), served at `/showings.ics`
```

- [ ] **Step 6: Commit**

```bash
git add app/web.py tests/test_ics.py AGENTS.md
git commit -m "Serve ICS calendar feed at /showings.ics"
```

---

### Task 3: Sidebar subscribe link

**Files:**
- Modify: `app/web.py` (template CSS + sidebar HTML)
- Test: `tests/test_ics.py` (append link test)

**Interfaces:**
- Consumes: the `/showings.ics` route from Task 2.
- Produces: an `<a class="cal" href="/showings.ics">` element in the `.telegram` sidebar.

- [ ] **Step 1: Write the failing test**

Append to `tests/test_ics.py`:

```python
def test_sidebar_has_subscribe_link(tmp_path):
    write_payload(tmp_path, [SHOWING])
    html = create_app(tmp_path).test_client().get("/").data.decode()
    assert 'class="cal"' in html
    assert 'href="/showings.ics"' in html
    assert "Subscribe to calendar" in html
```

- [ ] **Step 2: Run test to verify it fails**

Run: `.venv/bin/python -m pytest tests/test_ics.py::test_sidebar_has_subscribe_link -v`
Expected: FAIL — assertions on missing markup

- [ ] **Step 3: Add the link and styles**

In `app/web.py`'s template, add the link directly after the JOIN `<a>` inside `.telegram`:

```html
  <a class="cal" href="/showings.ics">📅 Subscribe to calendar</a>
```

In the template `<style>`, directly after the `.telegram a:hover{...}` rule, add:

```css
  .telegram a.cal{background:none;color:var(--dim);box-shadow:none;
   font-weight:400;letter-spacing:0;padding:.1rem 0;font-size:.8rem}
  .telegram a.cal:hover{background:none;color:var(--gold-bright)}
```

In the existing `@media (max-width:560px)` block, change the `.telegram` rule to add `flex-wrap:wrap`, and add a rule so the calendar link doesn't inherit the JOIN button's `margin-left:auto`:

```css
  @media (max-width:560px){
   .layout{flex-direction:column}
   .telegram{flex-direction:row;flex-wrap:wrap;text-align:left;position:static;padding:.7rem 1rem}
   .telegram a{margin-left:auto;width:auto}
   .telegram a.cal{margin-left:0;width:100%}
  }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `.venv/bin/python -m pytest -q`
Expected: all tests pass (62 total)

- [ ] **Step 5: Visual check**

Run: `./serve.sh restart`
Open http://localhost:8080 — the sidebar shows "📅 Subscribe to calendar" under the JOIN button; downloading `/showings.ics` opens a valid calendar file.
Then stop: `./serve.sh stop`

- [ ] **Step 6: Commit**

```bash
git add app/web.py tests/test_ics.py
git commit -m "Link the calendar feed in the sidebar"
```
