from datetime import datetime
from zoneinfo import ZoneInfo

from app.models import Showing
from app.notify import format_error, format_message, send_telegram

TZ = ZoneInfo("Europe/Vienna")


def make(cinema, movie, day, hour, minute, version, hall="", url="https://x"):
    return Showing(cinema, movie, datetime(2026, 7, day, hour, minute, tzinfo=TZ), version, hall, url)


def test_format_message_groups_by_cinema_and_formats_german():
    showings = [
        make("Megaplex PlusCity", "Die Odyssee", 20, 19, 45, "OV - IMAX 2D",
             url="https://www.megaplex.at/ticket/57419/539128"),
        make("Cineplexx Linz", "The Odyssey", 20, 19, 0, "OV", hall="Saal 6",
             url="https://cineplexx.at/film/die-odyssee"),
    ]
    msg = format_message(showings)
    lines = msg.split("\n")
    assert lines[0] == "🎬 Neue OV-Vorstellungen in Linz!"
    # cinemas sorted alphabetically: Cineplexx before Megaplex
    assert lines.index("Cineplexx Linz") < lines.index("Megaplex PlusCity")
    # 2026-07-20 is a Monday -> "Mo 20.07."
    assert "• The Odyssey (OV) — Mo 20.07., 19:00, Saal 6" in msg
    assert "• Die Odyssee (OV - IMAX 2D) — Mo 20.07., 19:45" in msg
    assert "https://cineplexx.at/film/die-odyssee" in msg
    assert "https://www.megaplex.at/ticket/57419/539128" in msg


def test_format_error():
    msg = format_error("Megaplex", ValueError("boom"))
    assert "Megaplex" in msg
    assert "boom" in msg


def test_send_telegram_posts_payload():
    calls = []

    class Resp:
        def raise_for_status(self):
            pass

    def fake_post(url, json, timeout):
        calls.append((url, json, timeout))
        return Resp()

    send_telegram("TOKEN", "123", "hello", post=fake_post)
    assert calls == [
        ("https://api.telegram.org/botTOKEN/sendMessage",
         {"chat_id": "123", "text": "hello"}, 20)
    ]
