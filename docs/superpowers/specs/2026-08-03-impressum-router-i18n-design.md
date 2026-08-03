# Impressum + Router + i18n — Design

Date: 2026-08-03

## Goal

- Legally-informed Impressum page for the public site.
- Client-side routing (more pages planned).
- i18n (English default, German second), incl. locale-correct date/time.
- Private, non-commercial hobby project.

## Decision: legal framing

Operator is a private individual; site is non-commercial. Impressum carries the
practicable minimum: name, town, email, "private non-commercial hobby project",
and a takedown note (content removed on short request). No full street address
(not ladungsfähig in the strict §5 ECG sense; accepted risk for a hobby site).

Operator data (both languages):
Klaus Gradinger · 4230 Pregarten · klaus.gradinger@gmail.com

## Stack

- `react-router-dom` v7 (BrowserRouter; backend already falls back to
  `index.html` for unknown paths, so `/impressum` works).
- `i18next` + `react-i18next` + `i18next-browser-languagedetector`.
- Default language `en`, fallback `en`; detection order localStorage →
  browser language; switch persisted.

## Structure

- `src/i18n.ts` — init.
- `src/locales/en.json`, `src/locales/de.json` — all UI strings.
- `src/main.tsx` — `<BrowserRouter>` + import `i18n`.
- `src/App.tsx` — `<Routes>`: `/` → `ShowingsPage`, `/impressum` → `ImpressumPage`.
- `src/pages/ShowingsPage.tsx` — current App content extracted.
- `src/pages/ImpressumPage.tsx` — the Impressum.
- `src/components/Footer.tsx` — Impressum link + `LanguageSwitcher`.
- `src/components/LanguageSwitcher.tsx` — EN | DE.

## API change (backend)

The API currently ships pre-formatted `date` ("Tue 04.08.") and `time` strings
plus a formatted `generatedAt`. For locale-correct display the frontend must
format dates itself:

- `showings[]`: replace `date`/`time` with `start` (ISO 8601 with Vienna offset).
- `generatedAt`: become ISO 8601.
- `meta_line` / `detail`: unchanged (scraped cinema data, German; not translated).

Frontend formats with `Intl.DateTimeFormat(locale)`.
Backend tests updated to the new shape.

## Testing

- Backend: web view-model tests assert the new `start`/`generatedAt` shape.
- Frontend: existing tests updated to run with the i18n instance; new tests for
  EN default, DE switch, and the `/impressum` route.

## Out of scope

- Translating scraped data (genre names, venue labels).
- URL-based locale prefixes (e.g. `/de/...`); plain paths only.
- Telegram message localization (German channel).
