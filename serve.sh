#!/usr/bin/env bash
# Start/stop the OV-Kino web UI locally (no check loop, no network fetches).
# Usage: ./serve.sh {start|stop|restart|status}
set -euo pipefail

cd "$(dirname "$0")"

DATA_DIR="${DATA_DIR:-./data}"
PIDFILE="$DATA_DIR/web.pid"
PORT="${PORT:-8080}"
PY=./.venv/bin/python
[ -x "$PY" ] || PY=python3

mkdir -p "$DATA_DIR"

seed_demo() {
    "$PY" - "$DATA_DIR" <<'EOF'
import sys
from datetime import datetime
from zoneinfo import ZoneInfo
from app.state import save_showings, load_showings

if load_showings(sys.argv[1]):
    raise SystemExit
save_showings(sys.argv[1], {
    "generated_at": datetime.now(ZoneInfo("Europe/Vienna")).isoformat(),
    "sources": {"cineplexx": "ok", "megaplex": "ok"},
    "showings": [
        {"cinema": "Megaplex PlusCity", "movie": "The Odyssey",
         "start": "2026-08-02T20:30:00+02:00", "version": "OV - IMAX 2D",
         "hall": "", "url": "https://www.megaplex.at/t/1"},
        {"cinema": "Cineplexx Linz", "movie": "The Odyssey",
         "start": "2026-08-02T19:00:00+02:00", "version": "OV",
         "hall": "Saal 7", "url": "https://cineplexx.at/f/x"},
    ],
})
EOF
}

is_running() {
    [ -f "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null
}

start() {
    if is_running; then
        echo "already running (pid $(cat "$PIDFILE"))"
        return
    fi
    seed_demo
    setsid nohup "$PY" - "$DATA_DIR" "$PORT" >/dev/null 2>&1 <<'EOF' &
import sys
from app.web import create_app
create_app(sys.argv[1]).run(host="0.0.0.0", port=int(sys.argv[2]))
EOF
    echo $! >"$PIDFILE"
    sleep 0.5
    if is_running; then
        echo "serving http://localhost:$PORT (pid $(cat "$PIDFILE"))"
    else
        echo "failed to start" >&2
        rm -f "$PIDFILE"
        exit 1
    fi
}

stop() {
    if ! is_running; then
        echo "not running"
        rm -f "$PIDFILE"
        return
    fi
    kill "$(cat "$PIDFILE")"
    rm -f "$PIDFILE"
    echo "stopped"
}

status() {
    if is_running; then
        echo "running (pid $(cat "$PIDFILE"), port $PORT)"
    else
        echo "not running"
    fi
}

case "${1:-}" in
    start)   start ;;
    stop)    stop ;;
    restart) stop; start ;;
    status)  status ;;
    *)       echo "usage: $0 {start|stop|restart|status}" >&2; exit 2 ;;
esac
