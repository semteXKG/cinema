from __future__ import annotations

import logging
import os
import threading
from datetime import datetime
from zoneinfo import ZoneInfo

from .checker import Config, run_check
from .fetchers import HttpClient
from .web import create_app

TZ = ZoneInfo("Europe/Vienna")
log = logging.getLogger("ov-watcher")


def scheduler_loop(stop: threading.Event, interval_s: float, run) -> None:
    while not stop.is_set():
        try:
            run()
        except Exception:
            log.exception("check run failed")
        stop.wait(interval_s)


def main() -> None:
    logging.basicConfig(
        level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s"
    )
    data_dir = os.environ.get("DATA_DIR", "/data")
    interval_s = float(os.environ.get("CHECK_INTERVAL_HOURS", "3")) * 3600
    port = int(os.environ.get("PORT", "8080"))
    config = Config(
        telegram_token=os.environ.get("TELEGRAM_BOT_TOKEN"),
        telegram_chat_id=os.environ.get("TELEGRAM_CHAT_ID"),
        sources=tuple(
            s.strip()
            for s in os.environ.get("SOURCES", "cineplexx,megaplex").split(",")
            if s.strip()
        ),
    )
    http = HttpClient(delay_s=0.5)
    stop = threading.Event()
    thread = threading.Thread(
        target=scheduler_loop,
        args=(stop, interval_s, lambda: run_check(http, data_dir, config, datetime.now(TZ))),
        daemon=True,
    )
    thread.start()
    log.info("starting web server on port %s", port)
    try:
        create_app(data_dir).run(host="0.0.0.0", port=port)
    finally:
        stop.set()


if __name__ == "__main__":
    main()
