# Web UI: "Cinematic Marquee" Redesign — Design

- Date: 2026-07-25
- Status: Approved by user (aesthetic direction + design)

## Problem

The web page (`app/web.py`) looks bland and generic: default-looking dark
theme, plain cards, system-ui everywhere. The user wants a distinctive
visual identity; grouping and content structure stay as they are.

## Goal

Restyle the page into a theater "Now Playing" board — **cinematic marquee**
direction (chosen by the user over modern-editorial and neo-brutalist
alternatives):

- Deep warm-black background (dark auditorium), amber/gold light accents.
- Glowing marquee header with "light bulb" dot rows.
- Cinema `<h2>` headings as gold "now showing" board headers with a thin
  double rule.
- Movie cards with a film-strip perforation strip on the left edge.
- Showing rows styled like ticket stubs (date tab, prominent time, dim
  detail), lifting with a warm glow on hover.
- One display font — *Limelight* (art-deco marquee face) — from Google
  Fonts with system-font fallbacks; the only external dependency.

All styling lives in the existing inline template in `app/web.py`.
Pure HTML/CSS; no JavaScript.

## Non-goals (YAGNI)

- No changes to grouping logic, data flow, routes, or any other module
  (`_group_showings`, fetchers, checker, state, notify stay untouched).
- No day-by-day regrouping (user explicitly wants cinema → movie kept).
- No light theme, no theme toggle, no responsive redesign beyond what the
  existing single-column layout already gives.

## Hard constraints from `tests/test_web.py`

Existing tests must pass **unchanged**. The template must keep these
byte-exact:

- `<h2>Cineplexx Linz</h2>` / `<h2>Megaplex PlusCity</h2>` — cinema names as
  plain `<h2>` with no extra attributes or nested markup.
- `<span class="badge">OV</span>` — badge markup exactly
  `<span class="badge">…</span>` (no added classes), rendered once per movie
  when versions match.
- `class="err"` / `class="ok"` spans for the source-status footer.
- Strings: `Noch keine Daten`, weekday+date format `Mo 20.07.`, times
  `19:00`, `>OV<` / `>OmU<` labels for mixed versions, detail strings
  (`Saal 7`, `Dolby Vision 2D`, …); the literal `OV - ` must not appear.
- Movie titles appear exactly once in the page (don't echo them elsewhere,
  e.g. in a count or header).

## Design details

**Palette** (warm theater tones, replacing the current cool blue-grey):

- Background: near-black with warm tint (e.g. `#0f0c09`), subtle radial
  vignette toward the edges.
- Cards: `#1c1712`-ish charcoal, thin muted-gold border.
- Primary accent: amber/gold (`#e8b34d`–`#f5c56b` range) for headings,
  times, hover glow.
- Text: warm off-white; details/meta in dim warm grey.
- ok/err: warm green / warm red.

**Header** — Title "OV-Kino Linz" (keep the 🎬) uppercase, letterspaced,
gold with a soft text-shadow glow, framed top and bottom by a row of small
"light bulbs" (CSS radial-gradient dot strips). Small dim subtitle
"Originalversionen in Linz".

**Cinema sections** — `<h2>` gold, letterspaced, small-caps feel, thin
double rule (border-top/bottom trick) beneath.

**Movie cards** — Left-edge film-strip perforation: a narrow vertical strip
of repeating dots via `repeating-linear-gradient`/`radial-gradient` on a
`::before` or padding + background layer. Movie title in the display font.
Badge restyled as a punched-ticket tag (gold background, dark text) —
markup unchanged.

**Showing rows** — Ticket-stub look: weekday+date in a small gold-outlined
tab, time in a brighter weight, detail dim; whole row remains a block-level
`<a>`; hover lifts the row (slight translate + warm glow, no layout shift).
Keep the existing `.when` / `.detail` spans (tests only pin content, but
minimal markup churn is preferred).

**Empty states & footer** — Same texts, dim styling; footer meta small;
source ok/err colors updated to the warm palette.

**Typography** — `<link>` to Google Fonts for *Limelight*, used for the
page title and movie titles; `font-display: swap`; fallback stack
`system-ui, sans-serif` everywhere else so the page renders fine offline.

**Meta refresh / lang / viewport** — unchanged.

## Tests

No test changes. Verification:

1. `.venv/bin/pytest -q` — whole suite green, `tests/test_web.py` untouched.
2. Render `/` against the real `data/showings.json` (Flask test client or
   dev server) and inspect the HTML for the new elements (marquee, film
   strip, font link) and correct escaping.
