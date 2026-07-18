import json

from tests.conftest import load_fixture
from app.fetchers import parse_cineplexx_showings


def load():
    movies = json.loads(load_fixture("cineplexx_movies.json"))
    odyssey_sessions = json.loads(load_fixture("cineplexx_sessions_odyssey.json"))
    return movies, {"HO00016814": odyssey_sessions}


def test_finds_only_ov_sessions_at_linz():
    movies, sessions = load()
    showings = parse_cineplexx_showings(movies, sessions)
    assert len(showings) == 6
    assert all(s.version == "OV" for s in showings)
    assert all(s.cinema == "Cineplexx Linz" for s in showings)


def test_showing_fields():
    movies, sessions = load()
    showings = parse_cineplexx_showings(movies, sessions)
    s = showings[0]
    assert s.movie == "The Odyssey"  # leading '*' stripped
    assert s.url == "https://cineplexx.at/film/die-odyssee"
    assert s.hall  # non-empty hall
    assert s.start.tzinfo is not None  # timezone-aware
    assert {x.start.day for x in showings} == {20, 21, 22, 23, 24, 26}


def test_sessions_from_other_cinemas_ignored():
    movies, sessions = load()
    showings = parse_cineplexx_showings(movies, sessions)
    # fixture contains hundreds of sessions from other cinemas (Apollo, Innsbruck, ...)
    assert len(showings) == 6
