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
    assert "Original Versions in Linz" in html
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
    assert '<p class="empty">No data yet' in html
    write_payload(tmp_path, [])
    html = create_app(tmp_path).test_client().get("/").data.decode()
    assert '<p class="empty">No OV showings found' in html


def test_telegram_panel(tmp_path):
    write_payload(tmp_path, one_showing())
    html = create_app(tmp_path).test_client().get("/").data.decode()
    assert 'class="telegram"' in html
    assert "@ov_linz" in html
    assert 'href="https://t.me/ov_linz"' in html
