"""Core data model and OV/OmU version matching."""

from __future__ import annotations

import re
from dataclasses import dataclass
from datetime import datetime

_VERSION_RE = re.compile(r"\b(OV|OmU|OmdU)\b")
_LANG_RE = re.compile(r"\(([^)]*)\)")


@dataclass(frozen=True)
class Showing:
    cinema: str
    movie: str
    start: datetime  # timezone-aware
    version: str
    hall: str
    url: str

    @property
    def key(self) -> str:
        return f"{self.cinema}|{self.movie}|{self.start.isoformat()}"


def is_english_ov_label(label: str) -> bool:
    """True if a version label marks an English original version."""
    if not _VERSION_RE.search(label):
        return False
    lang = _LANG_RE.search(label)
    if lang and "englisch" not in lang.group(1).lower():
        return False
    return True


def cineplexx_session_version(session: dict) -> str | None:
    """Return 'OV'/'OmU'/'OmdU' for an English OV session, else None."""
    tech = [t for group in session.get("technologies", []) for t in group]
    for label in tech:
        m = _VERSION_RE.search(label)
        if m and is_english_ov_label(label):
            return m.group(1)
    for attr in session.get("conceptAttributesNames") or []:
        if attr in ("OV", "OmU", "OmdU"):
            return attr
    return None


def megaplex_version(label: str) -> str | None:
    """Megaplex tags original-language showings with a leading 'OV'."""
    label = " ".join(label.split())
    return label if label.startswith("OV") else None
