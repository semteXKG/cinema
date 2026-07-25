from datetime import datetime
from zoneinfo import ZoneInfo

from app.state import save_showings
from app.web import create_app

TZ = ZoneInfo("Europe/Vienna")


def write_payload(data_dir, showings):
    save_showings(data_dir, {
        "generated_at": datetime(2026, 7, 18, 12, 0, tzinfo=TZ).isoformat(),
        "sources": {"cineplexx": "ok", "megaplex": "error"},
        "showings": showings,
    })


def test_healthz(tmp_path):
    client = create_app(tmp_path).test_client()
    resp = client.get("/healthz")
    assert resp.status_code == 200
    assert resp.data == b"ok"


def test_index_groups_by_cinema_and_movie(tmp_path):
    write_payload(tmp_path, [
        {"cinema": "Megaplex PlusCity", "movie": "Die Odyssee",
         "start": "2026-07-21T20:30:00+02:00", "version": "OV - IMAX 2D",
         "hall": "", "url": "https://www.megaplex.at/ticket/1"},
        {"cinema": "Cineplexx Linz", "movie": "The Odyssey",
         "start": "2026-07-20T19:00:00+02:00", "version": "OV",
         "hall": "Saal 7", "url": "https://cineplexx.at/film/die-odyssee"},
        {"cinema": "Cineplexx Linz", "movie": "The Odyssey",
         "start": "2026-07-21T20:00:00+02:00", "version": "OV",
         "hall": "Dolby Cinema", "url": "https://cineplexx.at/film/die-odyssee"},
    ])
    client = create_app(tmp_path).test_client()
    html = client.get("/").data.decode()
    # cinema headings, preferred order: Megaplex before Cineplexx
    assert html.index("<h2>Megaplex PlusCity</h2>") < html.index("<h2>Cineplexx Linz</h2>")
    # one card per movie: title appears exactly once for two showings
    assert html.count("The Odyssey") == 1
    # both showings present as rows with their own dates
    assert "Mo 20.07." in html and "Di 21.07." in html
    assert "19:00" in html and "20:00" in html
    # day-group headings are gone
    assert "Mo 20.07.2026" not in html
    # ticket link and source health still rendered
    assert "https://www.megaplex.at/ticket/1" in html
    assert 'class="err"' in html


def test_index_badge_once_when_versions_match(tmp_path):
    write_payload(tmp_path, [
        {"cinema": "Cineplexx Linz", "movie": "The Odyssey",
         "start": "2026-07-20T19:00:00+02:00", "version": "OV",
         "hall": "Saal 7", "url": "https://cineplexx.at/film/x"},
        {"cinema": "Cineplexx Linz", "movie": "The Odyssey",
         "start": "2026-07-21T20:00:00+02:00", "version": "OV",
         "hall": "Dolby Cinema", "url": "https://cineplexx.at/film/x"},
    ])
    html = create_app(tmp_path).test_client().get("/").data.decode()
    assert html.count('class="badge"') == 1
    assert '<span class="badge">OV</span>' in html
    assert "Saal 7" in html and "Dolby Cinema" in html


def test_index_per_showing_version_when_versions_differ(tmp_path):
    write_payload(tmp_path, [
        {"cinema": "Megaplex PlusCity", "movie": "Die Odyssee",
         "start": "2026-07-20T19:45:00+02:00", "version": "OV - Dolby Vision 2D",
         "hall": "", "url": "https://www.megaplex.at/ticket/1"},
        {"cinema": "Megaplex PlusCity", "movie": "Die Odyssee",
         "start": "2026-07-21T20:30:00+02:00", "version": "OV - IMAX 2D",
         "hall": "", "url": "https://www.megaplex.at/ticket/2"},
    ])
    html = create_app(tmp_path).test_client().get("/").data.decode()
    assert 'class="badge"' not in html
    assert "Dolby Vision 2D" in html and "IMAX 2D" in html
    assert "OV - " not in html


def test_index_mixed_versions_keep_plain_ov_label(tmp_path):
    write_payload(tmp_path, [
        {"cinema": "Cineplexx Linz", "movie": "Film X",
         "start": "2026-07-20T19:00:00+02:00", "version": "OV",
         "hall": "", "url": "https://cineplexx.at/film/x"},
        {"cinema": "Cineplexx Linz", "movie": "Film X",
         "start": "2026-07-21T19:00:00+02:00", "version": "OmU",
         "hall": "", "url": "https://cineplexx.at/film/x"},
    ])
    html = create_app(tmp_path).test_client().get("/").data.decode()
    assert 'class="badge"' not in html
    # plain "OV" showing keeps an explicit label so it stays distinguishable
    assert ">OV<" in html and ">OmU<" in html


def test_movies_ordered_by_earliest_showing(tmp_path):
    write_payload(tmp_path, [
        {"cinema": "Cineplexx Linz", "movie": "Late Film",
         "start": "2026-07-22T19:00:00+02:00", "version": "OV",
         "hall": "", "url": "https://cineplexx.at/film/late"},
        {"cinema": "Cineplexx Linz", "movie": "Early Film",
         "start": "2026-07-20T19:00:00+02:00", "version": "OV",
         "hall": "", "url": "https://cineplexx.at/film/early"},
    ])
    html = create_app(tmp_path).test_client().get("/").data.decode()
    assert html.index("Early Film") < html.index("Late Film")


def test_index_without_data(tmp_path):
    client = create_app(tmp_path).test_client()
    html = client.get("/").data.decode()
    assert "Noch keine Daten" in html
