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


def test_index_renders_showings_grouped_by_day(tmp_path):
    write_payload(tmp_path, [
        {"cinema": "Cineplexx Linz", "movie": "The Odyssey",
         "start": "2026-07-20T19:00:00+02:00", "version": "OV",
         "hall": "Saal 6", "url": "https://cineplexx.at/film/die-odyssee"},
        {"cinema": "Megaplex PlusCity", "movie": "Die Odyssee",
         "start": "2026-07-20T19:45:00+02:00", "version": "OV - IMAX 2D",
         "hall": "", "url": "https://www.megaplex.at/ticket/57419/539128"},
    ])
    client = create_app(tmp_path).test_client()
    html = client.get("/").data.decode()
    assert "The Odyssey" in html and "Die Odyssee" in html
    assert "OV - IMAX 2D" in html
    assert "Saal 6" in html
    assert "https://www.megaplex.at/ticket/57419/539128" in html
    assert "Mo 20.07.2026" in html  # day group header (Monday)
    assert 'class="err"' in html    # megaplex health shown as error


def test_index_without_data(tmp_path):
    client = create_app(tmp_path).test_client()
    html = client.get("/").data.decode()
    assert "Noch keine Daten" in html
