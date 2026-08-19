# Movie Ignore List — Design Spec

Date: 2026-08-19
Status: Draft

## Goal

Let users suppress notifications for movies they already know about. Each user maintains a per-(cinema, title) ignore list. Ignored movies do not produce notifications for that user, and collapse to a single one-liner in the web UI. The public `@ov_linz` Telegram channel is unaffected.

## Data Model

### New table: `movie_ignore`

```sql
CREATE TABLE movie_ignore (
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    cinema       TEXT NOT NULL,
    title        TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, cinema, title)
);
```

- `cinema` + `title` match the `movie` table's free-text columns (no `cinema` FK).
- `ON DELETE CASCADE` cleans up when a user is deleted.

### Schema notes
- `movie(cinema TEXT, title TEXT, … UNIQUE(cinema, title))` — confirmed in `0001_init.sql`.
- No `cinema` table exists.

## API

### Auth

All endpoints require authentication (existing session / Google OAuth).

### `PUT /api/movies/ignore`

Body:
```json
{ "cinema": "Cineplexx", "title": "Oppenheimer" }
```

Response `204 No Content` on success. Idempotent (`ON CONFLICT DO NOTHING`).

Errors: `400` invalid body, `401` unauthenticated, `500` server error.

### `DELETE /api/movies/ignore`

Body:
```json
{ "cinema": "Cineplexx", "title": "Oppenheimer" }
```

Response `204 No Content` on success. Idempotent (no-op if not ignored).

Errors: `400` invalid body, `401` unauthenticated, `500` server error.

### `GET /api/showings` (modified)

Currently public/anonymous. Becomes auth-aware:

- **Authenticated user** → `ignored: true/false` per movie, derived from `movie_ignore`.
- **Anonymous** → `ignored: false` for all movies.

`MovieView` gains a field:
```ts
ignored: boolean;
```

## Notification Filtering

In `backend/src/notification/batch.rs`, `handle_batch`:

```
load_batch_showings(...)
  → filter out showings where (cinema, title) is in user's movie_ignore set
  → if remaining showings empty → batch Skipped
  → else send
```

Filtering happens at **build time** (after loading), so showings queued before an ignore is set are also suppressed.

## Frontend

### `MovieCard.tsx`

**Normal state (not ignored):** unchanged — poster, title, badge, metaLine, showing links.

**Ignored state:**
- Full card body replaced with a single collapsed row: `"Oppenheimer" — Ignored · Cineplexx`
- **Unignore button** on that row (eye icon or "Unignore").
- Ignored movies **stay in place** (no reordering).

**Button visibility:** only shown to authenticated users; anonymous see normal cards with no button.

### `types.ts`

```ts
interface MovieView {
  // ...existing fields...
  ignored: boolean;
}
```

## Backend DB Helpers

New module `backend/src/ignore.rs` (or `backend/src/db/ignore.rs`):

```rust
async fn set_ignored(pool: &PgPool, user_id: &str, cinema: &str, title: &str) -> Result<()>
async fn unset_ignored(pool: &PgPool, user_id: &str, cinema: &str, title: &str) -> Result<()>
async fn ignored_keys(pool: &PgPool, user_id: &str) -> Result<HashSet<(String, String)>>
```

## Implementation Order

1. New migration `0004_movie_ignore.sql`
2. `backend/src/ignore.rs` — DB helpers
3. `PUT/DELETE /api/movies/ignore` handlers + route registration
4. `api_showings` — auth-aware, add `ignored` to `MovieView`
5. `handle_batch` — filter ignored at build time
6. `MovieView.ignored` in frontend types
7. `MovieCard` — collapsed one-liner + Unignore button
8. Backend `#[sqlx::test]` tests
9. Frontend tests

## Edge Cases

| Case | Behavior |
|------|----------|
| Already ignored → PUT | Idempotent, no-op |
| Un-ignore a never-ignored movie | Idempotent, no-op |
| All showings in batch ignored | Batch `Skipped`, no message sent |
| Anonymous viewer | Normal UI, `ignored=false`, no button |
| User deleted | `ON DELETE CASCADE` clears `movie_ignore` rows |
| Ignore a movie currently queued | Suppressed in next digest (build-time filter) |

## Testing

### Backend
- `set_ignored` / `unset_ignored` roundtrip
- Idempotent set (twice → no error)
- Ignored movie absent from batch message
- All-ignored batch → `Skipped`
- `api_showings` returns `ignored: true` for ignored movie for authed user, `false` for anonymous

### Frontend
- Button toggles between Ignore/Unignore
- Clicking Ignore collapses card to one-liner + "Ignored" label
- Clicking Unignore expands card back to normal
- API failure → error shown, state unchanged
- Anonymous user → no button, normal card
