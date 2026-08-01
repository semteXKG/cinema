import json

from tests.conftest import load_fixture
from app.fetchers import parse_cineplexx_showings
from app.models import MovieMeta


def load():
    movies = json.loads(load_fixture("cineplexx_movies.json"))
    odyssey_sessions = json.loads(load_fixture("cineplexx_sessions_odyssey.json"))
    return movies, {"HO00016814": odyssey_sessions}


def test_finds_only_ov_sessions_at_linz():
    movies, sessions = load()
    showings, _ = parse_cineplexx_showings(movies, sessions)
    assert len(showings) == 6
    assert all(s.version == "OV" for s in showings)
    assert all(s.cinema == "Cineplexx Linz" for s in showings)


def test_showing_fields():
    movies, sessions = load()
    showings, _ = parse_cineplexx_showings(movies, sessions)
    s = showings[0]
    assert s.movie == "The Odyssey"  # leading '*' stripped
    assert s.url == "https://cineplexx.at/film/die-odyssee"
    assert s.hall  # non-empty hall
    assert s.start.tzinfo is not None  # timezone-aware
    assert {x.start.day for x in showings} == {20, 21, 22, 23, 24, 26}


def test_sessions_from_other_cinemas_ignored():
    movies, sessions = load()
    showings, _ = parse_cineplexx_showings(movies, sessions)
    # fixture contains hundreds of sessions from other cinemas (Apollo, Innsbruck, ...)
    assert len(showings) == 6


def test_extracts_movie_metadata():
    movies, sessions = load()
    _, metas = parse_cineplexx_showings(movies, sessions)
    m = metas["The Odyssey"]
    assert m.runtime_min == 180
    assert m.genres == ("Abenteuer", "Historie")
    assert m.poster and m.poster.startswith("https://")


def test_metas_cover_all_movies_even_without_ov_sessions():
    movies, sessions = load()
    _, metas = parse_cineplexx_showings(movies, sessions)
    # one entry per movie in the list (17); checker filters to OV showings later
    assert len(metas) == len(movies) == 17


def test_cineplexx_meta_edge_values():
    # runTime 0 / missing genres / empty poster all degrade to None/()
    movies = [{"id": "X1", "title": "Odd", "runTime": 0,
               "genres": None, "posterImage": ""}]
    _, metas = parse_cineplexx_showings(movies, {})
    assert metas["Odd"] == MovieMeta(None, (), None)
