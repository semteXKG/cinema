"""One check run: fetch -> diff -> notify -> persist."""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path

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
    if new and can_notify:
        notifier(
            config.telegram_token,
            config.telegram_chat_id,
            notify.format_message(new),
        )
    for s in upcoming:
        state["seen"].setdefault(s.key, now.isoformat())
    state_mod.prune_state(state, now)
    state_mod.save_state(data_dir, state)
    wanted = {f"{s.cinema}|{s.movie}" for s in upcoming}
    movie_dicts = {
        key: state_mod.movie_meta_to_dict(m)
        for key, m in all_metas.items()
        if key in wanted
    }
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
