"""Persistent state: dedup keys, error-ping rate limits, showings.json."""

from __future__ import annotations

import json
from datetime import datetime, timedelta
from pathlib import Path

from .models import MovieMeta, Showing

_PRUNE_GRACE = timedelta(hours=6)


def _load_json(path: Path, default):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return default
    except (json.JSONDecodeError, OSError):
        path.rename(path.with_suffix(path.suffix + ".bak"))
        return default


def _save_json(path: Path, payload) -> None:
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(payload, ensure_ascii=False, indent=1), encoding="utf-8")
    tmp.replace(path)


def load_state(data_dir) -> dict:
    state = _load_json(Path(data_dir) / "state.json", {})
    if not isinstance(state, dict):
        state = {}
    state.setdefault("seen", {})
    state.setdefault("error_pings", {})
    return state


def save_state(data_dir, state: dict) -> None:
    _save_json(Path(data_dir) / "state.json", state)


def save_showings(data_dir, payload: dict) -> None:
    _save_json(Path(data_dir) / "showings.json", payload)


def load_showings(data_dir) -> dict | None:
    return _load_json(Path(data_dir) / "showings.json", None)


def showing_to_dict(s: Showing) -> dict:
    return {
        "cinema": s.cinema,
        "movie": s.movie,
        "start": s.start.isoformat(),
        "version": s.version,
        "hall": s.hall,
        "url": s.url,
    }


def movie_meta_to_dict(m: MovieMeta, poster_file: str | None = None) -> dict:
    return {
        "runtime_min": m.runtime_min,
        "genres": list(m.genres),
        "poster": m.poster,
        "poster_file": poster_file,
    }


def prune_state(state: dict, now: datetime) -> None:
    cutoff = now - _PRUNE_GRACE
    state["seen"] = {
        key: first_seen
        for key, first_seen in state["seen"].items()
        if datetime.fromisoformat(key.rsplit("|", 1)[1]) >= cutoff
    }
