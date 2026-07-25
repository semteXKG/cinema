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
    assert lines[0] == "🎬 <b>Neue OV-Vorstellungen in Linz</b>"
    # cinemas sorted alphabetically: Cineplexx before Megaplex
    assert lines.index("<b>Cineplexx Linz</b>") < lines.index("<b>Megaplex PlusCity</b>")
    # 2026-07-20 is a Monday -> "Mo 20.07."; title links to booking page
    assert (
        '• <a href="https://cineplexx.at/film/die-odyssee">The Odyssey</a> '
        "(OV) — Mo 20.07., 19:00 · Saal 6" in msg
    )
    assert (
        '• <a href="https://www.megaplex.at/ticket/57419/539128">Die Odyssee</a> '
        "(OV - IMAX 2D) — Mo 20.07., 19:45" in msg
    )
    # no bare URLs on their own lines anymore
    assert not any(line.startswith("http") for line in lines)


def test_format_message_escapes_html():
    showings = [
        make("Cineplexx Linz", "Fast & Furious <Final>", 20, 20, 0, "OV",
             url="https://x.at/film?a=1&b=2"),
    ]
    msg = format_message(showings)
    assert "Fast &amp; Furious &lt;Final&gt;" in msg
    assert 'href="https://x.at/film?a=1&amp;b=2"' in msg
    assert "Fast & Furious" not in msg


def test_format_error():
    msg = format_error("Megaplex", ValueError("boom"))
    assert "Megaplex" in msg
    assert "boom" in msg


def test_format_error_escapes_html():
    msg = format_error("Cineplexx", ValueError("<Response [500]> & stuff"))
    assert "&lt;Response [500]&gt; &amp; stuff" in msg
    assert "<Response [500]>" not in msg


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
         {"chat_id": "123", "text": "hello", "parse_mode": "HTML",
          "link_preview_options": {"is_disabled": True}}, 20)
    ]


def run_send(text):
    calls = []

    class Resp:
        def raise_for_status(self):
            pass

    def fake_post(url, json, timeout):
        calls.append(json["text"])
        return Resp()

    send_telegram("TOKEN", "123", text, post=fake_post)
    return calls


def test_send_telegram_splits_long_messages_on_line_boundaries():
    lines = [f"line {i} " + "x" * 90 for i in range(200)]  # ~20k chars total
    text = "\n".join(lines)
    chunks = run_send(text)
    assert len(chunks) > 1
    assert all(len(c) <= 4096 for c in chunks)
    # every line survives, in order, exactly once
    assert "\n".join(chunks).split("\n") == lines


def test_send_telegram_hard_wraps_single_overlong_line():
    text = "y" * 5000
    chunks = run_send(text)
    assert len(chunks) == 2
    assert all(len(c) <= 4096 for c in chunks)
    assert "".join(chunks) == text
