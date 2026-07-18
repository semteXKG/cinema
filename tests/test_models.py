from datetime import datetime
from zoneinfo import ZoneInfo

from app.models import (
    Showing,
    cineplexx_session_version,
    is_english_ov_label,
    megaplex_version,
)

TZ = ZoneInfo("Europe/Vienna")


def make_showing():
    return Showing(
        cinema="Cineplexx Linz",
        movie="The Odyssey",
        start=datetime(2026, 7, 20, 19, 0, tzinfo=TZ),
        version="OV",
        hall="Saal 6",
        url="https://cineplexx.at/film/die-odyssee",
    )


def test_showing_key():
    s = make_showing()
    assert s.key == f"Cineplexx Linz|The Odyssey|{s.start.isoformat()}"


def test_is_english_ov_label():
    assert is_english_ov_label("OV (Englisch)")
    assert is_english_ov_label("OmU (Englisch)")
    assert is_english_ov_label("OV")
    assert is_english_ov_label("OmU")
    assert not is_english_ov_label("2D")
    assert not is_english_ov_label("IMAX")
    assert not is_english_ov_label("OV (Französisch)")
    assert not is_english_ov_label("")


def test_cineplexx_session_version_from_technologies():
    session = {"technologies": [["2D", "OV (Englisch)"], []], "conceptAttributesNames": ["OV"]}
    assert cineplexx_session_version(session) == "OV"


def test_cineplexx_session_version_omu():
    session = {"technologies": [["2D", "OmU (Englisch)"], []], "conceptAttributesNames": []}
    assert cineplexx_session_version(session) == "OmU"


def test_cineplexx_session_version_german_dub():
    session = {"technologies": [["2D"], []], "conceptAttributesNames": ["Wertvoll"]}
    assert cineplexx_session_version(session) is None


def test_cineplexx_session_version_non_english_ov():
    session = {"technologies": [["2D", "OV (Französisch)"], []], "conceptAttributesNames": []}
    assert cineplexx_session_version(session) is None


def test_megaplex_version():
    assert megaplex_version("OV - IMAX 2D") == "OV - IMAX 2D"
    assert megaplex_version("  OV - Dolby Vision 2D  ") == "OV - Dolby Vision 2D"
    assert megaplex_version("Dolby Atmos 2D") is None
    assert megaplex_version("4DX 2D") is None
