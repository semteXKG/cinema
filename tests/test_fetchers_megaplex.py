from datetime import date

from tests.conftest import load_fixture
from app.fetchers import (
    MEGAPLEX_BASE,
    parse_megaplex_film_page,
    parse_megaplex_ov_links,
)

TODAY = date(2026, 7, 18)


def test_parse_ov_links_unique_and_absolute():
    html = load_fixture("megaplex_ov_program.html")
    links = parse_megaplex_ov_links(html)
    assert links == [
        f"{MEGAPLEX_BASE}/film/linz/die-odyssee/ov",
        f"{MEGAPLEX_BASE}/film/linz/insekten/ov",
        f"{MEGAPLEX_BASE}/film/linz/vaiana/ov",
    ]


def test_parse_film_page_showings():
    html = load_fixture("megaplex_film_ov.html")
    url = f"{MEGAPLEX_BASE}/film/linz/die-odyssee/ov"
    showings, _ = parse_megaplex_film_page(html, url, TODAY)
    assert len(showings) == 8
    assert all(s.cinema == "Megaplex PlusCity" for s in showings)
    assert all(s.movie == "Die Odyssee" for s in showings)
    assert all(s.version.startswith("OV") for s in showings)


def test_parse_film_page_dates_and_links():
    html = load_fixture("megaplex_film_ov.html")
    url = f"{MEGAPLEX_BASE}/film/linz/die-odyssee/ov"
    showings, _ = parse_megaplex_film_page(html, url, TODAY)
    first = showings[0]
    assert first.start.day == 18
    assert (first.start.hour, first.start.minute) == (19, 30)
    assert first.version == "OV - Dolby Vision 2D"
    assert first.url == f"{MEGAPLEX_BASE}/ticket/57419/539128"
    assert first.start.tzinfo is not None
    days = sorted(s.start.day for s in showings)
    assert days == [18, 18, 19, 20, 21, 22, 23, 28]


def test_parse_film_page_metadata():
    html = load_fixture("megaplex_film_ov.html")
    url = f"{MEGAPLEX_BASE}/film/linz/die-odyssee/ov"
    _, metas = parse_megaplex_film_page(html, url, TODAY)
    m = metas["Die Odyssee"]
    assert m.runtime_min == 173  # JSON-LD duration "PT173M"
    assert m.genres == ("Drama", "Action", "Abenteuer", "Fantasy")
    assert m.poster == "https://megaplexog.s3.eu-north-1.amazonaws.com/Odysee1.webp"


def test_parse_film_page_without_jsonld_has_no_meta():
    html = (
        "<html><body><h1>Other (Pluscity) - OV</h1>"
        "Aktuelles Kinoprogramm</body></html>"
    )
    showings, metas = parse_megaplex_film_page(html, "https://x", TODAY)
    assert showings == []
    assert metas == {}
