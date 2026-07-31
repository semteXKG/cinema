"""Tests for the ICS calendar feed renderer."""

from datetime import datetime, timezone

from app.ics import render_ics
from app.state import save_showings
from app.web import create_app

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
    assert "X-WR-CALNAME:OV Cinema Linz" in body
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


def test_sidebar_has_subscribe_link(tmp_path):
    write_payload(tmp_path, [SHOWING])
    html = create_app(tmp_path).test_client().get("/").data.decode()
    assert 'class="sidebar"' in html
    assert 'href="/showings.ics"' in html
    assert "SUBSCRIBE" in html
    assert "Add showings to your calendar" in html
