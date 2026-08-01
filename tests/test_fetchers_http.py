import json
from datetime import date

import pytest

from tests.conftest import load_fixture
from app.fetchers import (
    SourceError,
    fetch_cineplexx,
    fetch_megaplex,
)

TODAY = date(2026, 7, 18)


class FakeHttp:
    """Serves fixtures for known URLs; records requested URLs."""

    def __init__(self):
        self.json_routes = {}
        self.text_routes = {}
        self.requested = []

    def get_json(self, url, headers=None, params=None):
        self.requested.append(url)
        for key, value in self.json_routes.items():
            if key in url:
                return value
        raise SourceError(f"unexpected URL {url}")

    def get_text(self, url, headers=None):
        self.requested.append(url)
        for key, value in self.text_routes.items():
            if key in url:
                return value
        raise SourceError(f"unexpected URL {url}")


def make_cineplexx_http():
    http = FakeHttp()
    http.json_routes["/cinemasweb/1014/movies"] = json.loads(
        load_fixture("cineplexx_movies.json")
    )
    sessions = json.loads(load_fixture("cineplexx_sessions_odyssey.json"))
    http.json_routes["HO00016814/sessions"] = sessions
    http.json_routes["/sessions"] = []
    return http


def test_fetch_cineplexx_returns_ov_showings():
    showings, metas = fetch_cineplexx(make_cineplexx_http())
    assert len(showings) == 6
    assert all(s.cinema == "Cineplexx Linz" for s in showings)
    meta = metas["Cineplexx Linz|The Odyssey"]
    assert meta.runtime_min == 180
    # namespaced with the cinema name
    assert all(k.startswith("Cineplexx Linz|") for k in metas)


def test_fetch_cineplexx_empty_movie_list_is_source_error():
    http = FakeHttp()
    http.json_routes["/cinemasweb/1014/movies"] = []
    with pytest.raises(SourceError):
        fetch_cineplexx(http)


def test_fetch_megaplex_returns_ov_showings():
    http = FakeHttp()
    program = load_fixture("megaplex_ov_program.html")
    film = load_fixture("megaplex_film_ov.html")
    http.text_routes["/kinoprogramm/linz/"] = program
    http.text_routes["/film/linz/die-odyssee/ov"] = film
    # other linked OV films (insekten, vaiana) return a valid page without day-groups
    http.text_routes["/film/linz/"] = (
        "<html><body><h1>Other (Pluscity) - OV</h1>"
        "Aktuelles Kinoprogramm</body></html>"
    )
    showings, metas = fetch_megaplex(http, TODAY)
    assert len(showings) == 8
    assert all(s.cinema == "Megaplex PlusCity" for s in showings)
    # 14 program pages (one per day) + film pages
    program_calls = [u for u in http.requested if "/kinoprogramm/linz/" in u]
    assert len(program_calls) == 14
    # only the odyssey page carries JSON-LD metadata
    assert list(metas) == ["Megaplex PlusCity|Die Odyssee"]
    assert metas["Megaplex PlusCity|Die Odyssee"].runtime_min == 173


def test_fetch_megaplex_broken_page_is_source_error():
    http = FakeHttp()
    http.text_routes["/kinoprogramm/linz/"] = "<html><body>404</body></html>"
    with pytest.raises(SourceError):
        fetch_megaplex(http, TODAY)
