import hashlib
from datetime import datetime
from zoneinfo import ZoneInfo

from app.checker import Config, run_check
from app.models import MovieMeta, Showing
from app.state import load_showings, load_state

TZ = ZoneInfo("Europe/Vienna")
NOW = datetime(2026, 7, 18, 12, 0, tzinfo=TZ)


def make_showing(cinema="Cineplexx Linz", movie="The Odyssey", day=20):
    return Showing(cinema, movie, datetime(2026, 7, day, 19, 0, tzinfo=TZ),
                   "OV", "Saal 6", "https://x")


class FakeFetcher:
    def __init__(self, showings=None, metas=None, error=None):
        self.showings = showings or []
        self.metas = metas or {}
        self.error = error

    def __call__(self, http, today=None):
        if self.error:
            raise self.error
        return self.showings, self.metas


def test_new_showings_trigger_one_message(tmp_path):
    sent = []
    fetchers = {"cineplexx": FakeFetcher([make_showing()])}
    cfg = Config(telegram_token="T", telegram_chat_id="C", sources=("cineplexx",))
    result = run_check(None, tmp_path, cfg, NOW,
                       notifier=lambda t, c, text: sent.append(text),
                       fetcher_map=fetchers)
    assert result.new_showings == 1
    assert result.total_showings == 1
    assert len(sent) == 1
    assert "The Odyssey" in sent[0]

    # second run: same showing -> no new message
    result = run_check(None, tmp_path, cfg, NOW,
                       notifier=lambda t, c, text: sent.append(text),
                       fetcher_map=fetchers)
    assert result.new_showings == 0
    assert len(sent) == 1


def test_past_showings_are_dropped(tmp_path):
    sent = []
    past = make_showing(day=17)
    fetchers = {"cineplexx": FakeFetcher([past])}
    cfg = Config(telegram_token="T", telegram_chat_id="C", sources=("cineplexx",))
    result = run_check(None, tmp_path, cfg, NOW,
                       notifier=lambda t, c, text: sent.append(text),
                       fetcher_map=fetchers)
    assert result.total_showings == 0
    assert sent == []
    assert load_showings(tmp_path)["showings"] == []


def test_showings_json_written_with_health(tmp_path):
    fetchers = {"cineplexx": FakeFetcher([make_showing()])}
    cfg = Config(telegram_token=None, telegram_chat_id=None, sources=("cineplexx",))
    run_check(None, tmp_path, cfg, NOW, fetcher_map=fetchers)
    payload = load_showings(tmp_path)
    assert payload["sources"] == {"cineplexx": "ok"}
    assert payload["generated_at"] == NOW.isoformat()
    assert payload["showings"][0]["movie"] == "The Odyssey"


def test_source_error_sends_rate_limited_ping(tmp_path):
    from app.fetchers import SourceError

    sent = []
    err = SourceError("kaputt")
    fetchers = {"megaplex": FakeFetcher(error=err)}
    cfg = Config(telegram_token="T", telegram_chat_id="C", sources=("megaplex",))
    notifier = lambda t, c, text: sent.append(text)
    result = run_check(None, tmp_path, cfg, NOW, notifier=notifier, fetcher_map=fetchers)
    assert result.sources == {"megaplex": "error"}
    assert len(sent) == 1 and "kaputt" in sent[0]

    # same day: no second ping
    run_check(None, tmp_path, cfg, NOW, notifier=notifier, fetcher_map=fetchers)
    assert len(sent) == 1


def test_no_telegram_configured_still_writes_state(tmp_path):
    fetchers = {"cineplexx": FakeFetcher([make_showing()])}
    cfg = Config(telegram_token=None, telegram_chat_id=None, sources=("cineplexx",))
    run_check(None, tmp_path, cfg, NOW, fetcher_map=fetchers)
    assert make_showing().key in load_state(tmp_path)["seen"]


def test_showings_json_contains_movies_filtered_to_shown(tmp_path):
    metas = {
        "Cineplexx Linz|The Odyssey": MovieMeta(
            180, ("Abenteuer", "Historie"), "https://x/p.jpg"
        ),
        "Cineplexx Linz|Not Shown": MovieMeta(90, ("Drama",), "https://x/q.jpg"),
    }
    fetchers = {"cineplexx": FakeFetcher([make_showing()], metas=metas)}
    cfg = Config(telegram_token=None, telegram_chat_id=None, sources=("cineplexx",))
    run_check(None, tmp_path, cfg, NOW, fetcher_map=fetchers)
    movies = load_showings(tmp_path)["movies"]
    # meta for a movie without upcoming showings is filtered out
    assert list(movies) == ["Cineplexx Linz|The Odyssey"]
    assert movies["Cineplexx Linz|The Odyssey"] == {
        "runtime_min": 180,
        "genres": ["Abenteuer", "Historie"],
        "poster": "https://x/p.jpg",
        "poster_file": None,  # filled by the poster cache (Task 3)
    }


class FakePosterHttp:
    """Serves poster bytes; content=None simulates a download failure."""

    def __init__(self, content=b"\xff\xd8img"):
        self.content = content
        self.requested = []

    def get_bytes(self, url, headers=None):
        self.requested.append(url)
        if self.content is None:
            raise RuntimeError("download failed")
        return self.content


def make_meta(poster="https://cdn.example/poster.jpg"):
    return MovieMeta(180, ("Abenteuer", "Historie"), poster)


def test_posters_downloaded_and_referenced(tmp_path):
    http = FakePosterHttp()
    metas = {
        "Cineplexx Linz|The Odyssey": make_meta(),
        # filtered out (no such showing) -> must not trigger a download
        "Cineplexx Linz|Not Shown": make_meta("https://cdn.example/other.png"),
    }
    fetchers = {"cineplexx": FakeFetcher([make_showing()], metas=metas)}
    cfg = Config(telegram_token=None, telegram_chat_id=None, sources=("cineplexx",))
    run_check(http, tmp_path, cfg, NOW, fetcher_map=fetchers)
    assert http.requested == ["https://cdn.example/poster.jpg"]
    files = list((tmp_path / "posters").iterdir())
    assert len(files) == 1
    assert files[0].read_bytes() == b"\xff\xd8img"
    entry = load_showings(tmp_path)["movies"]["Cineplexx Linz|The Odyssey"]
    assert entry["poster_file"] == files[0].name
    # stable content-derived name: sha1(url)[:16] + ext
    assert files[0].name == (
        hashlib.sha1(b"https://cdn.example/poster.jpg").hexdigest()[:16] + ".jpg"
    )


def test_poster_cache_hit_skips_download(tmp_path):
    metas = {"Cineplexx Linz|The Odyssey": make_meta()}
    fetchers = {"cineplexx": FakeFetcher([make_showing()], metas=metas)}
    cfg = Config(telegram_token=None, telegram_chat_id=None, sources=("cineplexx",))
    run_check(FakePosterHttp(), tmp_path, cfg, NOW, fetcher_map=fetchers)
    # second run with a fresh http: file exists -> no download
    http2 = FakePosterHttp()
    run_check(http2, tmp_path, cfg, NOW, fetcher_map=fetchers)
    assert http2.requested == []
    entry = load_showings(tmp_path)["movies"]["Cineplexx Linz|The Odyssey"]
    assert entry["poster_file"] is not None


def test_poster_download_failure_is_best_effort(tmp_path):
    http = FakePosterHttp(content=None)
    metas = {"Cineplexx Linz|The Odyssey": make_meta()}
    fetchers = {"cineplexx": FakeFetcher([make_showing()], metas=metas)}
    cfg = Config(telegram_token=None, telegram_chat_id=None, sources=("cineplexx",))
    run_check(http, tmp_path, cfg, NOW, fetcher_map=fetchers)  # must not raise
    entry = load_showings(tmp_path)["movies"]["Cineplexx Linz|The Odyssey"]
    assert entry["poster_file"] is None
    assert not list((tmp_path / "posters").iterdir())


def test_prune_removes_unreferenced_posters(tmp_path):
    posters = tmp_path / "posters"
    posters.mkdir()
    (posters / "stale.jpg").write_bytes(b"old")
    metas = {"Cineplexx Linz|The Odyssey": make_meta()}
    fetchers = {"cineplexx": FakeFetcher([make_showing()], metas=metas)}
    cfg = Config(telegram_token=None, telegram_chat_id=None, sources=("cineplexx",))
    run_check(FakePosterHttp(), tmp_path, cfg, NOW, fetcher_map=fetchers)
    names = {f.name for f in posters.iterdir()}
    assert "stale.jpg" not in names
    assert len(names) == 1
