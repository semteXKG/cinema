# ICS calendar feed — design

Date: 2026-07-31

## Goal

Let visitors subscribe to all upcoming OV/OmU showings as a live calendar feed.
One `GET /showings.ics` route returns a `VCALENDAR` with one `VEVENT` per upcoming
showing; subscribed calendars (Google/Apple/Outlook) refresh automatically as the
watcher updates `showings.json`.

## Decisions (from brainstorming)

- Feed covers **all upcoming showings**, one event each — a live mirror of the web page.
- **Fixed 2h duration** per event (start times are scraped, no end times exist).
- Feed is advertised via a **"📅 Subscribe to calendar" link in the sidebar** under
  the Telegram JOIN button.
- Hand-rolled ICS generation, **no new dependencies** (format is simple; escaping
  rules are well-defined).

## Components

### `app/ics.py` (new)

Pure, independently testable module:

- `render_ics(showings: list[dict]) -> str` — builds the full `VCALENDAR` text from
  the showing dicts stored in `showings.json` (`cinema`, `movie`, `start`, `version`,
  `hall`, `url`).
- `VCALENDAR` header: `VERSION:2.0`, `PRODID:-//ov-kino-linz//EN`,
  `CALSCALE:GREGORIAN`, `METHOD:PUBLISH`, `X-WR-CALNAME:OV-Kino Linz`.
- Per showing, one `VEVENT`:
  - `UID` = `sha1(f"{cinema}|{movie}|{start}")` + `@ov-kino-linz` — reuses the
    existing dedup key shape from `models.Showing.key`, so events are stable across
    checks and calendars update them in place instead of duplicating.
  - `DTSTAMP` = generation time, UTC.
  - `DTSTART` = showing start converted to UTC, `YYYYMMDDTHHMMSSZ`.
  - `DTEND` = start + 2h, same format. UTC times avoid shipping a `VTIMEZONE` block.
  - `SUMMARY` = `{movie} ({version})` — version included since OV/OmU is the point.
  - `LOCATION` = `{cinema}` (+ `, {hall}` when hall is non-empty).
  - `DESCRIPTION` = version, hall, and booking URL as plain text.
  - `URL` = booking URL.
- RFC 5545 text escaping for `SUMMARY`/`LOCATION`/`DESCRIPTION`
  (`\` → `\\`, `;` → `\;`, `,` → `\,`, newline → `\n`) and 75-octet line folding
  (continuation lines start with a space).
- Line endings: CRLF per RFC 5545.

### `app/web.py`

- New route `GET /showings.ics`:
  - Loads the same payload as `/`; renders `render_ics(payload["showings"])`.
  - Response mimetype `text/calendar; charset=utf-8`.
  - Missing payload or empty showings → valid empty `VCALENDAR`, still HTTP 200
    (subscribers must not error).
- Sidebar: a "📅 Subscribe to calendar" link (`href="/showings.ics"`) below the JOIN
  button in the `.telegram` tower; styled as a subdued text link (not a second gold
  button).

## Data flow

`showings.json` (written by `checker.run_check`) → `GET /showings.ics` →
`render_ics` → subscriber's calendar app polls the URL periodically.

No state changes, no writes, no new config.

## Error handling

- Malformed `start` in a showing dict → skip that showing (don't fail the whole feed).
- All other fields are plain strings; escaping handles special characters.

## Testing (`tests/test_ics.py`)

- Route returns 200 + `text/calendar` content type.
- One `VEVENT` per showing in the payload.
- `DTEND` − `DTSTART` = 2h; times are UTC (`Z` suffix).
- UID is stable: same showing rendered twice → identical UID.
- Escaping: movie/cinema containing `,` `;` renders escaped.
- No payload → valid empty `VCALENDAR` (200).
- Sidebar contains the subscribe link (`/showings.ics`).

## Out of scope

- Per-showing "add to calendar" links (separate possible feature).
- Real runtimes via TMDB/OMDb.
- RSS feed.
