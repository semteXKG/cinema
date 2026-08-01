#!/usr/bin/env bash
# Start/stop the OV-Kino web UI locally (no check loop, no network fetches).
# Usage: ./serve.sh {start|stop|restart|status}
set -euo pipefail

cd "$(dirname "$0")"

DATA_DIR="$(realpath "${DATA_DIR:-./data}")"
PIDFILE="$DATA_DIR/web.pid"
PORT="${PORT:-8080}"
PY=./.venv/bin/python
[ -x "$PY" ] || PY=python3

mkdir -p "$DATA_DIR"

seed_demo() {
    "$PY" - "$DATA_DIR" <<'EOF'
import hashlib, struct, sys, zlib
from datetime import datetime
from zoneinfo import ZoneInfo
from pathlib import Path
from app.state import save_showings, load_showings

data_dir = Path(sys.argv[1])
if load_showings(data_dir):
    raise SystemExit

poster_url = "https://poster.example/odyssey.jpg"
poster_name = hashlib.sha1(poster_url.encode()).hexdigest()[:16] + ".png"
posters_dir = data_dir / "posters"
posters_dir.mkdir(parents=True, exist_ok=True)

# tiny valid 1×1 white PNG
img = struct.pack(">I", 13) + b"IHDR" + struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0)
img += struct.pack(">I", zlib.crc32(img[4:]) & 0xffffffff)
raw_data = b"\x00\xff\xff\xff"
idat = struct.pack(">I", len(zlib.compress(raw_data))) + b"IDAT" + zlib.compress(raw_data)
idat += struct.pack(">I", zlib.crc32(idat[4:]) & 0xffffffff)
iend = struct.pack(">I", 0) + b"IEND" + struct.pack(">I", zlib.crc32(b"IEND") & 0xffffffff)
poster_png = b"\x89PNG\r\n\x1a\n" + img + idat + iend

if not (posters_dir / poster_name).exists():
    (posters_dir / poster_name).write_bytes(poster_png)

save_showings(data_dir, {
    "generated_at": datetime.now(ZoneInfo("Europe/Vienna")).isoformat(),
    "sources": {"cineplexx": "ok", "megaplex": "ok"},
    "movies": {
        "Megaplex PlusCity|The Odyssey": {
            "runtime_min": 173,
            "genres": ["Drama", "Action", "Abenteuer", "Fantasy"],
            "poster": poster_url,
            "poster_file": poster_name,
        },
        "Cineplexx Linz|The Odyssey": {
            "runtime_min": 180,
            "genres": ["Abenteuer", "Historie"],
            "poster": poster_url,
            "poster_file": poster_name,
        },
    },
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
