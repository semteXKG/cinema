"""One check run: fetch -> diff -> notify -> persist."""

from __future__ import annotations

import hashlib
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from urllib.parse import urlsplit

from . import fetchers, notify
from . import state as state_mod
from .models import MovieMeta, Showing


@dataclass
class Config:
    telegram_token: str | None
    telegram_chat_id: str | None
    sources: tuple[str, ...] = ("cineplexx", "megaplex")


@dataclass
class CheckResult:
    new_showings: int = 0
    total_showings: int = 0
    sources: dict[str, str] = field(default_factory=dict)


def default_fetcher_map():
    return {
        "cineplexx": lambda http, today: fetchers.fetch_cineplexx(http),
        "megaplex": lambda http, today: fetchers.fetch_megaplex(http, today),
    }


_POSTER_HEADERS = {
    "User-Agent": (
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
        "(KHTML, like Gecko) Chrome/120.0 Safari/537.36"
    ),
}
_POSTER_EXTS = {".jpg", ".jpeg", ".png", ".webp"}


def _poster_filename(url: str) -> str:
    ext = Path(urlsplit(url).path).suffix.lower()
    if ext not in _POSTER_EXTS:
        ext = ".jpg"
    return hashlib.sha1(url.encode("utf-8")).hexdigest()[:16] + ext


def _cache_posters(http, metas: dict[str, MovieMeta], posters_dir: Path) -> dict:
    """Download missing posters; return key -> cached basename (None if missing)."""
    cached: dict[str, str | None] = {}
    for key, meta in metas.items():
        if not meta.poster:
            cached[key] = None
            continue
        name = _poster_filename(meta.poster)
        target = posters_dir / name
        if not target.exists():
            try:
                posters_dir.mkdir(parents=True, exist_ok=True)
                content = http.get_bytes(meta.poster, headers=_POSTER_HEADERS)
            except Exception:  # best-effort: retry on the next run
                cached[key] = None
                continue
            tmp = target.with_suffix(target.suffix + ".tmp")
            tmp.write_bytes(content)
            tmp.replace(target)
        cached[key] = name
    return cached


def _prune_posters(posters_dir: Path, keep: set[str]) -> None:
    if not posters_dir.is_dir():
        return
    for f in posters_dir.iterdir():
        if f.is_file() and f.name not in keep:
            f.unlink()


def run_check(
    http,
    data_dir,
    config: Config,
    now: datetime,
    notifier=None,
    fetcher_map=None,
) -> CheckResult:
    notifier = notifier or notify.send_telegram
    fetcher_map = fetcher_map or default_fetcher_map()
    data_dir = Path(data_dir)
    data_dir.mkdir(parents=True, exist_ok=True)
    state = state_mod.load_state(data_dir)

    can_notify = bool(config.telegram_token and config.telegram_chat_id)

    all_showings: list[Showing] = []
    all_metas: dict[str, MovieMeta] = {}
    health: dict[str, str] = {}
    for source in config.sources:
        try:
            showings, metas = fetcher_map[source](http, now.date())
            all_showings.extend(showings)
            all_metas.update(metas)
            health[source] = "ok"
        except fetchers.SourceError as e:
            health[source] = "error"
            already_pinged = state["error_pings"].get(source) == now.date().isoformat()
            if can_notify and not already_pinged:
                notifier(
                    config.telegram_token,
                    config.telegram_chat_id,
                    notify.format_error(source, e),
                )
                state["error_pings"][source] = now.date().isoformat()

    upcoming = [s for s in all_showings if s.start >= now]
    new = [s for s in upcoming if s.key not in state["seen"]]
    wanted = {f"{s.cinema}|{s.movie}" for s in upcoming}
    filtered_metas = {k: m for k, m in all_metas.items() if k in wanted}
    posters_dir = data_dir / "posters"
    poster_files = _cache_posters(http, filtered_metas, posters_dir)
    _prune_posters(posters_dir, {n for n in poster_files.values() if n})
    movie_dicts = {
        key: state_mod.movie_meta_to_dict(m, poster_files.get(key))
        for key, m in filtered_metas.items()
    }
    if new and can_notify:
        notifier(
            config.telegram_token,
            config.telegram_chat_id,
            notify.format_message(new, movies=movie_dicts),
        )
    for s in upcoming:
        state["seen"].setdefault(s.key, now.isoformat())
    state_mod.prune_state(state, now)
    state_mod.save_state(data_dir, state)
    state_mod.save_showings(
        data_dir,
        {
            "generated_at": now.isoformat(),
            "sources": health,
            "movies": movie_dicts,
            "showings": [
                state_mod.showing_to_dict(s)
                for s in sorted(upcoming, key=lambda x: (x.start, x.cinema))
            ],
        },
    )
    return CheckResult(len(new), len(upcoming), health)
