import json
from datetime import datetime, timedelta
from zoneinfo import ZoneInfo

from app.models import Showing
from app.state import (
    load_showings,
    load_state,
    prune_state,
    save_showings,
    save_state,
    showing_to_dict,
)

TZ = ZoneInfo("Europe/Vienna")
NOW = datetime(2026, 7, 18, 12, 0, tzinfo=TZ)


def make_showing(day, hour=19):
    return Showing(
        cinema="Cineplexx Linz",
        movie="The Odyssey",
        start=datetime(2026, 7, day, hour, 0, tzinfo=TZ),
        version="OV",
        hall="Saal 6",
        url="https://cineplexx.at/film/die-odyssee",
    )


def test_load_state_missing_returns_default(tmp_path):
    state = load_state(tmp_path)
    assert state == {"seen": {}, "error_pings": {}}


def test_state_roundtrip(tmp_path):
    state = {"seen": {"k": "2026-07-18T00:00:00+02:00"}, "error_pings": {}}
    save_state(tmp_path, state)
    assert load_state(tmp_path) == state


def test_load_state_corrupt_backs_up(tmp_path):
    (tmp_path / "state.json").write_text("{not json", encoding="utf-8")
    state = load_state(tmp_path)
    assert state == {"seen": {}, "error_pings": {}}
    assert (tmp_path / "state.json.bak").exists()


def test_showings_roundtrip(tmp_path):
    payload = {"generated_at": NOW.isoformat(), "sources": {}, "showings": []}
    save_showings(tmp_path, payload)
    assert load_showings(tmp_path) == payload


def test_load_showings_missing(tmp_path):
    assert load_showings(tmp_path) is None


def test_showing_to_dict_serializable():
    d = showing_to_dict(make_showing(20))
    assert json.dumps(d)
    assert d["start"] == "2026-07-20T19:00:00+02:00"


def test_prune_state_drops_past_keys():
    old = make_showing(17).key   # >6h in the past relative to NOW
    future = make_showing(20).key
    state = {"seen": {old: NOW.isoformat(), future: NOW.isoformat()}, "error_pings": {}}
    prune_state(state, NOW)
    assert old not in state["seen"]
    assert future in state["seen"]
