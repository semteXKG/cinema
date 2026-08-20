# Notification Rules Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace each user's single per-channel notification frequency with an ordered, first-match-wins per-user rule list (cinema + features (any-of) + optional title substring → frequency), routed into per-frequency batches.

**Architecture:** New `notification_rule` table + `showing.features` column + `notification_batch.frequency` column. The existing digest/batch/send engine is reused; the only engine change is keying open batches by `(user, layer, frequency)`. Matching runs in Rust at routing time. The public `@ov_linz` broadcast is untouched.

**Tech Stack:** Rust (axum, sqlx/Postgres, chrono), React 19 + Vite + TypeScript, Vitest. Migrations under `backend/migrations/`. Tests: `cargo test` (needs `DATABASE_URL` + `docker compose up -d db`) and `npm test`.

Spec: `docs/superpowers/specs/2026-08-20-notification-rules-design.md`.

## Global Constraints

- Feature vocabulary (exact tokens): `OV`, `OmU`, `OmdU`, `2D`, `3D`, `IMAX`, `Atmos`, `DolbyCinema`, `4DX`.
- Frequency vocabulary: `never` | `immediately` | `1`..`7`.
- Feature matching: **OR (any-of)**; empty rule features = no constraint; no rule match → `never` (suppress).
- Per-user rules; a rule's frequency applies to both enabled layers (enablement replaces per-layer frequency).
- ≤ 32 rules per user; `titleSubstring` ≤ 200 chars; `cinemaId` null or in `cinema` table.
- API camelCase JSON; auth via `ov_session` cookie (`AuthUser`); 401 without session.
- Public `@ov_linz` broadcast (checker step 5) must remain unchanged.
- `#[sqlx::test(migrations = "./migrations")]` runs all migrations on a fresh per-test DB.

## File Structure

Backend:
- `backend/migrations/0006_notification_rules.sql` — **create**: schema.
- `backend/src/models.rs` — **modify**: `Showing.features` + `extract_features()`.
- `backend/src/fetchers/{cineplexx,megaplex}.rs` — **modify**: populate features.
- `backend/src/db.rs` — **modify**: `insert_showing` gains `features`.
- `backend/src/notification/rules.rs` — **create**: `Rule`, `MatchableShowing`, `matches()`, `first_match()`.
- `backend/src/notification/db.rs` — **modify**: prefs reshape, rule CRUD, batch-frequency helpers, `list_active_users_with_rules`, `load_matchable_showings`.
- `backend/src/notification/batch.rs` — **modify**: `route_showing_for_users`; `handle_batch` passes frequency; rewrite tests.
- `backend/src/notification/mod.rs` — **modify**: `pub mod rules;`.
- `backend/src/notification/api.rs` — **modify**: prefs reshape; rules routes + validation.
- `backend/src/checker.rs` — **modify**: step 6 routes via rules.

Frontend:
- `frontend/src/types.ts` — **modify**: prefs → enablement; rule/cinema types + `FEATURES`.
- `frontend/src/api/preferences.ts` — **modify**: new shape + `fetchRules`/`saveRules`.
- `frontend/src/pages/PreferencesPage.tsx` — **modify**: enable toggles + rule editor.
- `frontend/src/pages/PreferencesPage.test.tsx` — **modify**.
- `frontend/src/locales/{en,de}.json` — **modify**: rule-editor keys.

---

### Task 1: Migration 0006 — schema

**Files:**
- Create: `backend/migrations/0006_notification_rules.sql`

**Interfaces:**
- Produces: `showing.features TEXT[]`, `notification_batch.frequency TEXT`, `notification_rule` table, `notification_preferences.{email_enabled,telegram_enabled}` (old frequency columns dropped), `idx_batch_open_unique` on `(user_id, layer, frequency) WHERE status='pending'`.

- [ ] **Step 1: Write the migration**

`backend/migrations/0006_notification_rules.sql`:

```sql
-- 1. showing.features (computed at fetch time; existing rows stay '{}')
ALTER TABLE showing ADD COLUMN features TEXT[] NOT NULL DEFAULT '{}';

-- 2. notification_batch.frequency
ALTER TABLE notification_batch ADD COLUMN frequency TEXT NOT NULL DEFAULT 'immediately';

-- 3. Backfill each pending batch's frequency from its OWN layer's old pref
UPDATE notification_batch b SET frequency = COALESCE(
  CASE WHEN b.layer = 'email' THEN p.email_frequency ELSE p.telegram_frequency END,
  'never')
FROM notification_preferences p
WHERE p.user_id = b.user_id AND b.status = 'pending';

-- 4. Rekey the open-batch unique index to include frequency
DROP INDEX IF EXISTS idx_batch_open_unique;
CREATE UNIQUE INDEX idx_batch_open_unique
  ON notification_batch(user_id, layer, frequency) WHERE status = 'pending';

-- 5. notification_rule table
CREATE TABLE notification_rule (
  id              BIGSERIAL PRIMARY KEY,
  user_id         BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  position        INT NOT NULL,
  cinema_id       BIGINT REFERENCES cinema(id),
  features        TEXT[] NOT NULL DEFAULT '{}',
  title_substring TEXT,
  frequency       TEXT NOT NULL,
  created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (user_id, position)
);
CREATE INDEX idx_notification_rule_user ON notification_rule(user_id, position);

-- 6. preferences: add enablement columns
ALTER TABLE notification_preferences
  ADD COLUMN email_enabled    BOOL NOT NULL DEFAULT false,
  ADD COLUMN telegram_enabled BOOL NOT NULL DEFAULT false;

-- 7. Backfill enablement from the old frequency columns
UPDATE notification_preferences
  SET email_enabled    = (email_frequency <> 'never'),
      telegram_enabled = (telegram_frequency <> 'never');

-- 8. Seed one catch-all rule per user with any non-never frequency,
--    cross-layer urgency (immediately preferred)
INSERT INTO notification_rule (user_id, position, cinema_id, features, title_substring, frequency)
SELECT user_id, 0, NULL, '{}', NULL,
  CASE WHEN email_frequency = 'immediately' OR telegram_frequency = 'immediately'
       THEN 'immediately'
       WHEN email_frequency <> 'never' THEN email_frequency
       WHEN telegram_frequency <> 'never' THEN telegram_frequency
       ELSE '3' END
FROM notification_preferences
WHERE email_frequency <> 'never' OR telegram_frequency <> 'never';

-- 9. Drop the old per-layer frequency columns
ALTER TABLE notification_preferences
  DROP COLUMN email_frequency,
  DROP COLUMN telegram_frequency;
```

- [ ] **Step 2: Verify migrations apply**

With `docker compose up -d db` and `DATABASE_URL` set, confirm the new migration applies on a fresh DB by running any existing sqlx test:
Run: `cd backend && cargo test --lib db::tests::showing_insert_dedups -- --nocapture`
Expected: PASS (sqlx applies all migrations incl. 0006 on the per-test DB). If it fails on `0006`, fix the SQL.

- [ ] **Step 3: Commit**

```bash
git add backend/migrations/0006_notification_rules.sql
git commit -m "migration: notification rules schema (features, batch.frequency, rules table, prefs enablement)"
```

---

### Task 2: Feature extraction + `Showing.features`

**Files:**
- Modify: `backend/src/models.rs`
- Test: `backend/src/models.rs`

**Interfaces:**
- Produces: `pub fn extract_features(text: &str) -> Vec<String>` (deduped, vocab-ordered); `Showing.features: Vec<String>`.

- [ ] **Step 1: Write the failing tests**

Add to `backend/src/models.rs` `mod tests`:

```rust
    fn features(text: &str) -> Vec<String> {
        super::extract_features(text)
    }

    #[test]
    fn extract_megaplex_imax_2d() {
        assert_eq!(features("OV - IMAX 2D"), vec!["OV", "IMAX", "2D"]);
    }

    #[test]
    fn extract_dolby_atmos_is_atmos_not_dolbycinema() {
        assert_eq!(features("OV - Dolby Atmos"), vec!["OV", "Atmos"]);
    }

    #[test]
    fn extract_dolby_cinema_and_vision() {
        assert_eq!(features("Dolby Cinema 2D"), vec!["DolbyCinema", "2D"]);
        assert_eq!(features("OV - Dolby Vision"), vec!["OV", "DolbyCinema"]);
    }

    #[test]
    fn extract_omu_and_omdu() {
        assert_eq!(features("OmU"), vec!["OmU"]);
        assert_eq!(features("OmdU (Englisch)"), vec!["OmdU"]);
    }

    #[test]
    fn extract_unknown_yields_empty() {
        assert!(features(" regulärer Text ").is_empty());
    }

    #[test]
    fn extract_dedupes_and_preserves_vocab_order() {
        assert_eq!(features("imax 3D IMAX"), vec!["IMAX", "3D"]);
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test --lib models::tests::extract_ -- --nocapture`
Expected: FAIL (`extract_features` not defined).

- [ ] **Step 3: Implement `extract_features` + field**

Add to `backend/src/models.rs` after `megaplex_version`, before `#[cfg(test)]`:

```rust
static FEATURE_TOKENS: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
    vec![
        (r"\bOV\b", "OV"),
        (r"\bOmdU\b", "OmdU"),
        (r"\bOmU\b", "OmU"),
        (r"\bIMAX\b", "IMAX"),
        (r"\bDolby\s+Atmos\b|\bAtmos\b", "Atmos"),
        (r"\bDolby\s+Cinema\b|\bDolby\s+Vision\b", "DolbyCinema"),
        (r"\b3D\b", "3D"),
        (r"\b2D\b", "2D"),
        (r"\b4DX\b", "4DX"),
    ]
});

static FEATURE_RES: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    FEATURE_TOKENS
        .iter()
        .map(|(pat, tok)| (Regex::new(&format!("(?i){pat}")).unwrap(), *tok))
        .collect()
});

/// Extract normalized feature tags from combined text (version + hall +
/// Cineplexx technologies/attributes). Deduped, vocab-ordered.
pub fn extract_features(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (re, tok) in FEATURE_RES.iter() {
        if re.is_match(text) && !out.iter().any(|t| t == tok) {
            out.push((*tok).to_string());
        }
    }
    out
}
```

Add `features: Vec<String>` to the `Showing` struct (last field). Update the `make_showing()` helper in `mod tests` to add `features: vec![],`.

- [ ] **Step 4: Run to verify pass**

Run: `cd backend && cargo test --lib models::tests -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Fix downstream `Showing` literals (compile)**

`Showing` is also constructed in `notify.rs` `make()`, `checker.rs` `make_showing()`, `ics.rs` tests, `fetchers/*`. Build to find them:
Run: `cd backend && cargo build --tests 2>&1 | grep "missing field"`
Add `features: vec![]` to each test fixture `Showing { ... }` and to the fetcher `Showing { ... }` (temporarily; Task 3 replaces fetchers with real extraction).

- [ ] **Step 6: Commit**

```bash
git add backend/src/models.rs backend/src/notify.rs backend/src/checker.rs backend/src/ics.rs backend/src/fetchers/
git commit -m "models: extract normalized features and add Showing.features"
```

---

### Task 3: Fetchers produce features

**Files:**
- Modify: `backend/src/fetchers/megaplex.rs`
- Modify: `backend/src/fetchers/cineplexx.rs`
- Test: both files

**Interfaces:**
- Produces: each `Showing` from the fetchers carries real `features`.

- [ ] **Step 1: Write failing tests**

In `backend/src/fetchers/megaplex.rs` add (or extend) `#[cfg(test)] mod tests`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn megaplex_features_from_version() {
        let version = megaplex_version("OV - IMAX 2D").unwrap();
        let combined = format!("{version} ");
        assert_eq!(
            crate::models::extract_features(&combined),
            vec!["OV", "IMAX", "2D"]
        );
    }
}
```

In `backend/src/fetchers/cineplexx.rs` add:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cineplexx_features_from_session_arrays() {
        let session = json!({
            "technologies": [["2D", "OV (Englisch)", "IMAX"], []],
            "conceptAttributesNames": ["OV"],
            "screenName": "Saal 6"
        });
        let text = cineplexx_feature_text(&session, "OV");
        assert_eq!(
            crate::models::extract_features(&text),
            vec!["OV", "IMAX", "2D"]
        );
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test --lib fetchers:: -- --nocapture`
Expected: FAIL (`cineplexx_feature_text` not defined).

- [ ] **Step 3: Implement**

In `backend/src/fetchers/cineplexx.rs` add near `cineplexx_session_version`:

```rust
/// Combined text from which `extract_features` pulls tags: `technologies` +
/// `conceptAttributesNames` arrays, screen name, and the resolved OV version.
/// Keeps the detail the fetcher previously discarded.
pub fn cineplexx_feature_text(session: &serde_json::Value, version: &str) -> String {
    let mut parts: Vec<String> = vec![version.to_string()];
    if let Some(screen) = session.get("screenName").and_then(|v| v.as_str()) {
        parts.push(screen.to_string());
    }
    if let Some(groups) = session.get("technologies").and_then(|t| t.as_array()) {
        for group in groups.iter().filter_map(|g| g.as_array()) {
            for label in group.iter().filter_map(|l| l.as_str()) {
                parts.push(label.to_string());
            }
        }
    }
    if let Some(attrs) = session.get("conceptAttributesNames").and_then(|a| a.as_array()) {
        for attr in attrs.iter().filter_map(|a| a.as_str()) {
            parts.push(attr.to_string());
        }
    }
    parts.join(" ")
}
```

In `parse_cineplexx_showings`, the `Showing { ... }` `features: vec![]` becomes:

```rust
                    features: crate::models::extract_features(
                        &cineplexx_feature_text(session, &version)
                    ),
```

In `backend/src/fetchers/megaplex.rs`, in the `Showing { ... }` in `parse_megaplex_film_page`, replace `features: vec![]` with (add a local `let combined = format!("{version} {hall}");` above the struct literal; `hall` is `String::new()`):

```rust
                features: crate::models::extract_features(&combined),
```

- [ ] **Step 4: Run to verify pass**

Run: `cd backend && cargo test --lib fetchers:: -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/fetchers/megaplex.rs backend/src/fetchers/cineplexx.rs
git commit -m "fetchers: populate Showing.features from version/session arrays"
```

---

### Task 4: `insert_showing` carries features

**Files:**
- Modify: `backend/src/db.rs` (`insert_showing`)
- Modify: `backend/src/checker.rs` (caller)
- Test: `backend/src/db.rs`

**Interfaces:**
- Produces: `pub async fn insert_showing(pool, movie_id, start, version, hall, url, first_seen, features: &[String]) -> sqlx::Result<Option<i64>>`.

- [ ] **Step 1: Write the failing test**

Add to `backend/src/db.rs` `mod tests`:

```rust
    #[sqlx::test(migrations = "./migrations")]
    async fn showing_insert_persists_features(pool: PgPool) {
        let mid = upsert_movie(&pool, "Cineplexx Linz", "F1", None, &[], None, None)
            .await
            .unwrap();
        assert!(insert_showing(
            &pool, mid, at(19), "OV - IMAX 2D", "", "https://x", at(12),
            &["OV".into(), "IMAX".into(), "2D".into()],
        ).await.unwrap().is_some());
        let row: (Vec<String>,) =
            sqlx::query_as("SELECT features FROM showing WHERE movie_id = $1")
                .bind(mid).fetch_one(&pool).await.unwrap();
        assert_eq!(row.0, vec!["OV".to_string(), "IMAX".to_string(), "2D".to_string()]);
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test --lib db::tests::showing_insert_persists_features -- --nocapture`
Expected: FAIL (arity mismatch).

- [ ] **Step 3: Update `insert_showing` + callers**

In `backend/src/db.rs`:

```rust
pub async fn insert_showing(
    pool: &PgPool,
    movie_id: i64,
    start: DateTime<Utc>,
    version: &str,
    hall: &str,
    url: &str,
    first_seen: DateTime<Utc>,
    features: &[String],
) -> sqlx::Result<Option<i64>> {
    let row: Option<(i64,)> = sqlx::query_as(
        "INSERT INTO showing (movie_id, start, version, hall, url, first_seen_at, features)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (movie_id, start) DO NOTHING
         RETURNING id",
    )
    .bind(movie_id).bind(start).bind(version).bind(hall).bind(url).bind(first_seen).bind(features)
    .fetch_optional(pool).await?;
    Ok(row.map(|r| r.0))
}
```

Update every other `insert_showing(...)` call to pass `&[]` (test fixtures) except `checker.rs`. In `backend/src/checker.rs` `run_check`:

```rust
        if let Some(showing_id) = db::insert_showing(
            ctx.pool, movie_id, s.start, &s.version, &s.hall, &s.url, now, &s.features,
        ).await?
        {
```

Fix sites: `backend/src/db.rs` tests (add `&[]`), `backend/src/checker.rs` tests (`make_showing`-based, `&[]`), `backend/src/web.rs` tests (`&[]`), `backend/src/ics.rs` tests (`&[]`), `backend/src/notification/batch.rs` `make_showing` helper (`&[]`).

- [ ] **Step 4: Run to verify pass**

Run: `cd backend && cargo test --lib -- --nocapture`
Expected: PASS (fix any remaining arity errors).

- [ ] **Step 5: Commit**

```bash
git add backend/src/db.rs backend/src/checker.rs backend/src/notification/batch.rs backend/src/ics.rs backend/src/web.rs
git commit -m "db: persist showing.features via insert_showing"
```

---

### Task 5: Rule matching (pure)

**Files:**
- Create: `backend/src/notification/rules.rs`
- Modify: `backend/src/notification/mod.rs` (`pub mod rules;`)
- Test: `backend/src/notification/rules.rs`

**Interfaces:**
- Produces:
  - `pub struct Rule { pub cinema_id: Option<i64>, pub features: Vec<String>, pub title_substring: Option<String>, pub frequency: String }`
  - `pub struct MatchableShowing { pub showing_id: i64, pub cinema_id: i64, pub features: Vec<String>, pub title: String }`
  - `pub fn matches(rule: &Rule, s: &MatchableShowing) -> bool`
  - `pub fn first_match(rules: &[Rule], s: &MatchableShowing) -> Option<&str>`

- [ ] **Step 1: Write the failing tests**

Create `backend/src/notification/rules.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn rule(cinema_id: Option<i64>, features: &[&str], title: Option<&str>, freq: &str) -> Rule {
        Rule {
            cinema_id,
            features: features.iter().map(|s| s.to_string()).collect(),
            title_substring: title.map(|s| s.to_string()),
            frequency: freq.to_string(),
        }
    }

    fn showing(cinema_id: i64, features: &[&str], title: &str) -> MatchableShowing {
        MatchableShowing {
            showing_id: 1,
            cinema_id,
            features: features.iter().map(|s| s.to_string()).collect(),
            title: title.to_string(),
        }
    }

    #[test]
    fn empty_features_matches_anything() {
        let r = rule(None, &[], None, "3");
        assert!(matches(&r, &showing(1, &["IMAX"], "X")));
        assert!(matches(&r, &showing(2, &[], "Y")));
    }

    #[test]
    fn any_of_overlap_matches() {
        let r = rule(None, &["IMAX", "Atmos"], None, "immediately");
        assert!(matches(&r, &showing(1, &["OV", "Atmos"], "X")));
        assert!(matches(&r, &showing(1, &["IMAX", "2D"], "X")));
        assert!(!matches(&r, &showing(1, &["OV", "2D"], "X")));
    }

    #[test]
    fn cinema_specific_and_any() {
        let r = rule(Some(7), &[], None, "immediately");
        assert!(matches(&r, &showing(7, &[], "X")));
        assert!(!matches(&r, &showing(8, &[], "X")));
    }

    #[test]
    fn title_substring_case_insensitive() {
        let r = rule(None, &[], Some("odyssey"), "immediately");
        assert!(matches(&r, &showing(1, &[], "The Odyssey")));
        assert!(!matches(&r, &showing(1, &[], "F1")));
    }

    #[test]
    fn title_substring_trimmed_empty_is_any() {
        let r = rule(None, &[], Some("   "), "3");
        assert!(matches(&r, &showing(1, &[], "Anything")));
    }

    #[test]
    fn first_match_wins_in_order() {
        let rules = vec![
            rule(Some(7), &["IMAX"], None, "immediately"),
            rule(None, &[], None, "3"),
        ];
        assert_eq!(first_match(&rules, &showing(7, &["IMAX"], "X")), Some("immediately"));
        assert_eq!(first_match(&rules, &showing(8, &["OV"], "Y")), Some("3"));
    }

    #[test]
    fn no_match_returns_none() {
        let rules = vec![rule(Some(7), &[], None, "immediately")];
        assert_eq!(first_match(&rules, &showing(9, &[], "X")), None);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test --lib notification::rules:: -- --nocapture`
Expected: FAIL (module not declared).

- [ ] **Step 3: Implement**

Prepend to `backend/src/notification/rules.rs` (above `#[cfg(test)]`):

```rust
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct Rule {
    pub cinema_id: Option<i64>,
    pub features: Vec<String>,
    pub title_substring: Option<String>,
    pub frequency: String,
}

#[derive(Debug, Clone)]
pub struct MatchableShowing {
    pub showing_id: i64,
    pub cinema_id: i64,
    pub features: Vec<String>,
    pub title: String,
}

/// A rule matches iff: cinema is any OR equal; features is empty OR overlaps
/// (any-of); title_substring is empty/whitespace OR contained (case-insensitive).
pub fn matches(rule: &Rule, s: &MatchableShowing) -> bool {
    if let Some(cid) = rule.cinema_id {
        if cid != s.cinema_id {
            return false;
        }
    }
    if !rule.features.is_empty() {
        let need: HashSet<&str> = rule.features.iter().map(|s| s.as_str()).collect();
        let have: HashSet<&str> = s.features.iter().map(|s| s.as_str()).collect();
        if need.intersection(&have).next().is_none() {
            return false;
        }
    }
    if let Some(t) = rule.title_substring.as_deref() {
        let t = t.trim();
        if !t.is_empty() && !s.title.to_lowercase().contains(&t.to_lowercase()) {
            return false;
        }
    }
    true
}

/// First matching rule's frequency, in order. None if no rule matches (= never).
pub fn first_match<'a>(rules: &'a [Rule], s: &MatchableShowing) -> Option<&'a str> {
    for r in rules {
        if matches(r, s) {
            return Some(&r.frequency);
        }
    }
    None
}
```

In `backend/src/notification/mod.rs` add `pub mod rules;`.

- [ ] **Step 4: Run to verify pass**

Run: `cd backend && cargo test --lib notification::rules:: -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/notification/rules.rs backend/src/notification/mod.rs
git commit -m "notification: pure rule matching (any-of features, first-match-wins)"
```

---

### Task 6: Rule CRUD in DB

**Files:**
- Modify: `backend/src/notification/db.rs`
- Test: `backend/src/notification/db.rs`

**Interfaces:**
- Produces:
  - `#[derive(sqlx::FromRow)] pub struct NotificationRule { id, user_id, position: i32, cinema_id: Option<i64>, features: Vec<String>, title_substring: Option<String>, frequency: String }`
  - `pub struct RuleInput { cinema_id: Option<i64>, features: Vec<String>, title_substring: Option<String>, frequency: String }`
  - `pub async fn list_rules(pool, user_id) -> sqlx::Result<Vec<NotificationRule>>`
  - `pub async fn replace_rules(pool, user_id, &[RuleInput]) -> sqlx::Result<Vec<NotificationRule>>` (tx delete+insert, position=index)
  - `pub async fn list_cinemas(pool) -> sqlx::Result<Vec<(i64, String)>>`

- [ ] **Step 1: Write the failing tests**

Add to `backend/src/notification/db.rs` `mod tests`:

```rust
    async fn make_rule_user(pool: &PgPool) -> i64 {
        crate::db::find_or_create_user(pool, "email", "rules@x.com", "rules@x.com")
            .await.unwrap()
    }

    fn input(cinema_id: Option<i64>, features: &[&str], title: Option<&str>, freq: &str) -> RuleInput {
        RuleInput {
            cinema_id,
            features: features.iter().map(|s| s.to_string()).collect(),
            title_substring: title.map(|s| s.to_string()),
            frequency: freq.to_string(),
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn replace_rules_inserts_in_order(pool: PgPool) {
        let uid = make_rule_user(&pool).await;
        let inserted = replace_rules(&pool, uid, &[
            input(Some(1), &["IMAX", "Atmos"], None, "immediately"),
            input(None, &[], None, "3"),
        ]).await.unwrap();
        assert_eq!(inserted.len(), 2);
        assert_eq!(inserted[0].position, 0);
        assert_eq!(inserted[0].features, vec!["IMAX", "Atmos"]);
        assert_eq!(inserted[1].position, 1);
        assert_eq!(inserted[1].frequency, "3");
        let listed = list_rules(&pool, uid).await.unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, inserted[0].id);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn replace_rules_is_replacement(pool: PgPool) {
        let uid = make_rule_user(&pool).await;
        replace_rules(&pool, uid, &[input(None, &[], None, "3")]).await.unwrap();
        let replaced = replace_rules(&pool, uid, &[
            input(Some(1), &["IMAX"], None, "immediately")
        ]).await.unwrap();
        assert_eq!(replaced.len(), 1);
        assert_eq!(replaced[0].frequency, "immediately");
        assert_eq!(list_rules(&pool, uid).await.unwrap().len(), 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn list_cinemas_returns_known(pool: PgPool) {
        let cinemas = list_cinemas(&pool).await.unwrap();
        let names: Vec<String> = cinemas.iter().map(|(_, n)| n.clone()).collect();
        assert!(names.contains(&"Cineplexx Linz".to_string()));
        assert!(names.contains(&"Megaplex PlusCity".to_string()));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test --lib notification::db::tests::replace_rules -- --nocapture`
Expected: FAIL (types missing).

- [ ] **Step 3: Implement**

Add to `backend/src/notification/db.rs` (after existing structs):

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct NotificationRule {
    pub id: i64,
    pub user_id: i64,
    pub position: i32,
    pub cinema_id: Option<i64>,
    pub features: Vec<String>,
    pub title_substring: Option<String>,
    pub frequency: String,
}

#[derive(Debug, Clone)]
pub struct RuleInput {
    pub cinema_id: Option<i64>,
    pub features: Vec<String>,
    pub title_substring: Option<String>,
    pub frequency: String,
}

pub async fn list_rules(pool: &PgPool, user_id: i64) -> sqlx::Result<Vec<NotificationRule>> {
    sqlx::query_as(
        "SELECT id, user_id, position, cinema_id, features, title_substring, frequency
         FROM notification_rule WHERE user_id = $1 ORDER BY position",
    ).bind(user_id).fetch_all(pool).await
}

pub async fn replace_rules(
    pool: &PgPool,
    user_id: i64,
    input: &[RuleInput],
) -> sqlx::Result<Vec<NotificationRule>> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM notification_rule WHERE user_id = $1")
        .bind(user_id).execute(&mut *tx).await?;
    for (i, r) in input.iter().enumerate() {
        sqlx::query(
            "INSERT INTO notification_rule
               (user_id, position, cinema_id, features, title_substring, frequency)
             VALUES ($1, $2, $3, $4, NULLIF($5, ''), $6)",
        )
        .bind(user_id).bind(i as i32).bind(r.cinema_id).bind(&r.features)
        .bind(r.title_substring.as_deref()).bind(&r.frequency)
        .execute(&mut *tx).await?;
    }
    let rows = sqlx::query_as::<_, NotificationRule>(
        "SELECT id, user_id, position, cinema_id, features, title_substring, frequency
         FROM notification_rule WHERE user_id = $1 ORDER BY position",
    ).bind(user_id).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows)
}

pub async fn list_cinemas(pool: &PgPool) -> sqlx::Result<Vec<(i64, String)>> {
    sqlx::query_as("SELECT id, name FROM cinema ORDER BY id").fetch_all(pool).await
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd backend && cargo test --lib notification::db::tests::replace_rules notification::db::tests::list_cinemas -- --nocapture`
Expected: PASS (existing prefs tests still fail until Task 7 — expected, don't fix here).

- [ ] **Step 5: Commit**

```bash
git add backend/src/notification/db.rs
git commit -m "notification/db: rule CRUD (list, replace, cinemas)"
```

---

### Task 7: Preferences reshape (enablement)

**Files:**
- Modify: `backend/src/notification/db.rs` (`NotificationPreferences`, `PreferenceUpdate`, `get_preferences`, `upsert_preferences`; delete `list_active_preferences`)
- Test: `backend/src/notification/db.rs`

**Interfaces:**
- Produces:
  - `NotificationPreferences { user_id, email_enabled: bool, telegram_enabled: bool, telegram_handle: Option<String>, telegram_chat_id: Option<String>, digest_anchor: DateTime<Utc>, digest_hour: i32, updated_at: DateTime<Utc> }`
  - `PreferenceUpdate { email_enabled: Option<bool>, telegram_enabled: Option<bool>, telegram_handle: Option<String>, digest_anchor: Option<DateTime<Utc>>, digest_hour: Option<i32> }`
  - `get_preferences`, `upsert_preferences` rewritten for the new columns.

- [ ] **Step 1: Write the failing tests**

In `backend/src/notification/db.rs` `mod tests`, replace the `prefs_for` helper and the `preferences_defaults_and_upsert`, `get_preferences_defaults_derive_anchor_from_user`, `list_active_preferences_filters` tests (they reference `email_frequency`/`telegram_frequency` which no longer exist):

```rust
    async fn prefs_for(
        pool: &PgPool,
        uid: i64,
        email_enabled: bool,
        telegram_enabled: bool,
        handle: Option<&str>,
        digest_anchor: Option<DateTime<Utc>>,
        digest_hour: Option<i32>,
    ) {
        upsert_preferences(pool, uid, PreferenceUpdate {
            email_enabled: Some(email_enabled),
            telegram_enabled: Some(telegram_enabled),
            telegram_handle: handle.map(|s| s.to_string()),
            digest_anchor,
            digest_hour,
        }).await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn preferences_defaults_and_upsert(pool: PgPool) {
        let uid = make_user(&pool, "a@b.com").await;
        let prefs = get_preferences(&pool, uid).await.unwrap();
        assert!(!prefs.email_enabled);
        assert!(!prefs.telegram_enabled);
        assert!(prefs.telegram_handle.is_none());
        assert_eq!(prefs.digest_hour, 9);

        let updated = upsert_preferences(&pool, uid, PreferenceUpdate {
            email_enabled: Some(true),
            telegram_enabled: Some(false),
            telegram_handle: Some("@MyHandle".into()),
            digest_anchor: None,
            digest_hour: Some(10),
        }).await.unwrap();
        assert!(updated.email_enabled);
        assert!(!updated.telegram_enabled);
        assert_eq!(updated.telegram_handle.as_deref(), Some("myhandle"));
    }
```

Delete `list_active_preferences_filters` (replaced by `list_active_users_with_rules` test in Task 9).

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test --lib notification::db::tests::preferences_defaults_and_upsert -- --nocapture`
Expected: FAIL (struct fields renamed).

- [ ] **Step 3: Implement**

Replace the `NotificationPreferences` struct:

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct NotificationPreferences {
    pub user_id: i64,
    pub email_enabled: bool,
    pub telegram_enabled: bool,
    pub telegram_handle: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub digest_anchor: DateTime<Utc>,
    pub digest_hour: i32,
    #[allow(dead_code)]
    pub updated_at: DateTime<Utc>,
}
```

Replace `PreferenceUpdate`:

```rust
#[derive(Debug, Default)]
pub struct PreferenceUpdate {
    pub email_enabled: Option<bool>,
    pub telegram_enabled: Option<bool>,
    pub telegram_handle: Option<String>,
    pub digest_anchor: Option<DateTime<Utc>>,
    pub digest_hour: Option<i32>,
}
```

Rewrite `get_preferences` (default no-row returns `email_enabled: false, telegram_enabled: false`):

```rust
pub async fn get_preferences(pool: &PgPool, user_id: i64) -> sqlx::Result<NotificationPreferences> {
    let created_at: Option<(DateTime<Utc>,)> =
        sqlx::query_as("SELECT created_at FROM users WHERE id = $1")
            .bind(user_id).fetch_optional(pool).await?;
    let Some((created_at,)) = created_at else {
        return Err(sqlx::Error::RowNotFound);
    };
    let row = sqlx::query_as::<_, NotificationPreferences>(
        "SELECT user_id, email_enabled, telegram_enabled, telegram_handle,
                telegram_chat_id, digest_anchor, digest_hour, updated_at
         FROM notification_preferences WHERE user_id = $1",
    ).bind(user_id).fetch_optional(pool).await?;
    Ok(match row {
        Some(prefs) => prefs,
        None => NotificationPreferences {
            user_id,
            email_enabled: false,
            telegram_enabled: false,
            telegram_handle: None,
            telegram_chat_id: None,
            digest_anchor: created_at,
            digest_hour: 9,
            updated_at: created_at,
        },
    })
}
```

Rewrite `upsert_preferences` (handle normalization + chat-id-clear logic unchanged; frequency fields → booleans):

```rust
pub async fn upsert_preferences(
    pool: &PgPool,
    user_id: i64,
    dto: PreferenceUpdate,
) -> sqlx::Result<NotificationPreferences> {
    let existing = get_preferences(pool, user_id).await?;
    let new_handle = dto.telegram_handle
        .map(|raw| raw.trim().trim_start_matches('@').to_lowercase())
        .filter(|h| !h.is_empty());
    let clear_chat = new_handle.is_none() || new_handle.as_deref() != existing.telegram_handle.as_deref();
    let chat_id = if clear_chat { None } else { existing.telegram_chat_id.clone() };
    let email_enabled = dto.email_enabled.unwrap_or(existing.email_enabled);
    let telegram_enabled = dto.telegram_enabled.unwrap_or(existing.telegram_enabled);
    let digest_anchor = dto.digest_anchor.unwrap_or(existing.digest_anchor);
    let digest_hour = dto.digest_hour.unwrap_or(existing.digest_hour);
    sqlx::query_as::<_, NotificationPreferences>(
        "INSERT INTO notification_preferences
           (user_id, email_enabled, telegram_enabled, telegram_handle,
            telegram_chat_id, digest_anchor, digest_hour, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, now())
         ON CONFLICT (user_id) DO UPDATE SET
           email_enabled    = EXCLUDED.email_enabled,
           telegram_enabled = EXCLUDED.telegram_enabled,
           telegram_handle  = EXCLUDED.telegram_handle,
           telegram_chat_id  = EXCLUDED.telegram_chat_id,
           digest_anchor    = EXCLUDED.digest_anchor,
           digest_hour      = EXCLUDED.digest_hour,
           updated_at       = now()
         RETURNING user_id, email_enabled, telegram_enabled, telegram_handle,
                   telegram_chat_id, digest_anchor, digest_hour, updated_at",
    )
    .bind(user_id).bind(email_enabled).bind(telegram_enabled).bind(new_handle)
    .bind(chat_id).bind(digest_anchor).bind(digest_hour)
    .fetch_one(pool).await
}
```

Delete `list_active_preferences` (replaced in Task 9 by `list_active_users_with_rules`). Update `DueBatch` stays as-is (`frequency: String` now sourced from `b.frequency`). Delete the `upsert_clears_chat_id_on_handle_change_or_clear` and `get_preferences_defaults_derive_anchor_from_user` tests' frequency assertions — keep them but switch field names (`email_enabled`/`telegram_enabled`). The `get_due_batches_*` tests (Task 8) will be updated separately.

- [ ] **Step 4: Run to verify pass**

Run: `cd backend && cargo test --lib notification::db::tests::preferences_defaults_and_upsert -- --nocapture`
Expected: PASS. Other db tests referencing old fields may still fail (Tasks 8/9 fix them).

- [ ] **Step 5: Commit**

```bash
git add backend/src/notification/db.rs
git commit -m "notification/db: preferences reshape to per-layer enablement"
```

---

### Task 8: Batch frequency column + helpers

**Files:**
- Modify: `backend/src/notification/db.rs` (`get_or_create_open_batch`, `create_empty_batch`, `get_due_batches`; update tests)
- Test: `backend/src/notification/db.rs`

**Interfaces:**
- Produces:
  - `pub async fn get_or_create_open_batch(pool, user_id, layer, frequency) -> sqlx::Result<i64>`
  - `pub async fn create_empty_batch(pool, user_id, layer, frequency) -> sqlx::Result<i64>`
  - `get_due_batches` reads `b.frequency` (no preferences CASE).
- `delete_open_batch(pool, user_id, layer)` unchanged (deletes all pending for user+layer across frequencies).

- [ ] **Step 1: Write the failing tests**

Add to `backend/src/notification/db.rs` `mod tests`:

```rust
    #[sqlx::test(migrations = "./migrations")]
    async fn open_batches_are_per_frequency(pool: PgPool) {
        let uid = make_user(&pool, "freq@x.com").await;
        let a = get_or_create_open_batch(&pool, uid, "email", "immediately").await.unwrap();
        let b = get_or_create_open_batch(&pool, uid, "email", "3").await.unwrap();
        let a2 = get_or_create_open_batch(&pool, uid, "email", "immediately").await.unwrap();
        assert_ne!(a, b);
        assert_eq!(a, a2, "idempotent per (user, layer, frequency)");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_due_batches_reads_batch_frequency(pool: PgPool) {
        let uid = make_user(&pool, "due@x.com").await;
        prefs_for(&pool, uid, true, false, None, None, None).await;
        let imm = get_or_create_open_batch(&pool, uid, "email", "immediately").await.unwrap();
        let three = get_or_create_open_batch(&pool, uid, "email", "3").await.unwrap();
        let due = get_due_batches(&pool, Utc::now()).await.unwrap();
        let ids: Vec<i64> = due.iter().map(|d| d.batch_id).collect();
        // immediately is always due; 3-day is due only at digest (not now)
        assert!(ids.contains(&imm));
        assert!(!ids.contains(&three));
        let imm_row = due.iter().find(|d| d.batch_id == imm).unwrap();
        assert_eq!(imm_row.frequency, "immediately");
    }
```

Update the existing `get_due_batches_returns_pending_and_retryable_failed` test: replace its `upsert_preferences(... email_frequency: Some("immediately") ...)` calls with the new `email_enabled: Some(true)` shape and seed a catch-all rule so frequency resolves. Simpler: insert pending batches directly via `get_or_create_open_batch(&pool, email_user, "email", "immediately")` etc., and assert `frequency` comes from the batch. Rewrite that test to:

```rust
    #[sqlx::test(migrations = "./migrations")]
    async fn get_due_batches_returns_pending_and_retryable_failed(pool: PgPool) {
        let email_user = make_user(&pool, "i1@x.com").await;
        let telegram_user = make_user(&pool, "i2@x.com").await;
        let sent_user = make_user(&pool, "i3@x.com").await;

        let pending_email = get_or_create_open_batch(&pool, email_user, "email", "immediately").await.unwrap();
        let pending_tg = get_or_create_open_batch(&pool, telegram_user, "telegram", "3").await.unwrap();

        let recent_failed = create_empty_batch(&pool, email_user, "telegram", "immediately").await.unwrap();
        mark_batch_failed(&pool, recent_failed, "x").await.unwrap();

        let retry_failed = create_empty_batch(&pool, telegram_user, "email", "3").await.unwrap();
        mark_batch_failed(&pool, retry_failed, "y").await.unwrap();
        sqlx::query("UPDATE notification_batch SET updated_at = now() - interval '3 hours' WHERE id = $1")
            .bind(retry_failed).execute(&pool).await.unwrap();

        let sent = create_empty_batch(&pool, sent_user, "telegram", "immediately").await.unwrap();
        mark_batch_sent(&pool, sent).await.unwrap();

        let due = get_due_batches(&pool, Utc::now()).await.unwrap();
        let ids: Vec<i64> = due.iter().map(|d| d.batch_id).collect();
        assert!(ids.contains(&pending_email));
        assert!(ids.contains(&pending_tg));
        assert!(ids.contains(&retry_failed));
        assert!(!ids.contains(&recent_failed));
        assert!(!ids.contains(&sent));

        assert_eq!(due.iter().find(|d| d.batch_id == pending_email).unwrap().frequency, "immediately");
        assert_eq!(due.iter().find(|d| d.batch_id == pending_tg).unwrap().frequency, "3");
        assert_eq!(due.iter().find(|d| d.batch_id == retry_failed).unwrap().frequency, "3");
    }
```

Update `get_due_batches_high_error_count_retryable_without_overflow`: change its `INSERT INTO notification_batch ...` to include `frequency` (e.g. `'immediately'`) and `layer`/`status`/`error_count` as before; assertion unchanged.

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test --lib notification::db::tests::open_batches_are_per_frequency -- --nocapture`
Expected: FAIL (`get_or_create_open_batch` arity).

- [ ] **Step 3: Implement**

In `backend/src/notification/db.rs`:

```rust
pub async fn get_or_create_open_batch(
    pool: &PgPool,
    user_id: i64,
    layer: &str,
    frequency: &str,
) -> sqlx::Result<i64> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO notification_batch (user_id, layer, frequency) VALUES ($1, $2, $3)
         ON CONFLICT (user_id, layer, frequency) WHERE status = 'pending'
         DO UPDATE SET updated_at = now()
         RETURNING id",
    ).bind(user_id).bind(layer).bind(frequency).fetch_one(pool).await?;
    Ok(row.0)
}

pub async fn create_empty_batch(
    pool: &PgPool,
    user_id: i64,
    layer: &str,
    frequency: &str,
) -> sqlx::Result<i64> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO notification_batch (user_id, layer, frequency) VALUES ($1, $2, $3) RETURNING id",
    ).bind(user_id).bind(layer).bind(frequency).fetch_one(pool).await?;
    Ok(row.0)
}
```

Rewrite `get_due_batches` to read `b.frequency`:

```rust
pub async fn get_due_batches(pool: &PgPool, now: DateTime<Utc>) -> sqlx::Result<Vec<DueBatch>> {
    sqlx::query_as(
        "SELECT b.id AS batch_id, b.user_id, b.layer, b.frequency AS frequency,
                COALESCE(p.digest_anchor, u.created_at) AS digest_anchor,
                COALESCE(p.digest_hour, 9) AS digest_hour,
                b.created_at, b.error_count
         FROM notification_batch b
         JOIN users u ON u.id = b.user_id
         LEFT JOIN notification_preferences p ON p.user_id = b.user_id
         WHERE b.status = 'pending'
            OR (b.status = 'failed'
                AND b.updated_at + make_interval(hours => LEAST(POWER(2, LEAST(b.error_count, 5))::int, 24)) <= $1)
         ORDER BY b.id",
    ).bind(now).fetch_all(pool).await
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd backend && cargo test --lib notification::db::tests:: -- --nocapture`
Expected: PASS for all db tests (prefs + batch). Fix any remaining old-frequency references.

- [ ] **Step 5: Commit**

```bash
git add backend/src/notification/db.rs
git commit -m "notification/db: per-frequency batches (open/create/get_due)"
```

---

### Task 9: Active users + rules, matchable showings

**Files:**
- Modify: `backend/src/notification/db.rs`
- Test: `backend/src/notification/db.rs`

**Interfaces:**
- Produces:
  - `pub struct UserRules { pub user_id: i64, pub email_enabled: bool, pub telegram_enabled: bool, pub telegram_chat_id: Option<String>, pub digest_anchor: DateTime<Utc>, pub digest_hour: i32, pub rules: Vec<crate::notification::rules::Rule> }`
  - `pub async fn list_active_users_with_rules(pool) -> sqlx::Result<Vec<UserRules>>`
  - `pub async fn load_matchable_showings(pool, &[i64]) -> sqlx::Result<Vec<crate::notification::rules::MatchableShowing>>`
- Consumes: `Rule`/`MatchableShowing` (Task 5), `notification_rule`/`showing.features`/`cinema` (Task 1).

- [ ] **Step 1: Write the failing tests**

Add to `backend/src/notification/db.rs` `mod tests`:

```rust
    #[sqlx::test(migrations = "./migrations")]
    async fn list_active_users_with_rules_filters_inactive(pool: PgPool) {
        let active = make_user(&pool, "act@x.com").await;
        let inactive = make_user(&pool, "inact@x.com").await;
        let tg_unverified = make_user(&pool, "tg@x.com").await;
        prefs_for(&pool, active, true, false, None, None, None).await;
        prefs_for(&pool, inactive, false, false, None, None, None).await;
        prefs_for(&pool, tg_unverified, false, true, Some("h"), None, None).await;
        replace_rules(&pool, active, &[RuleInput {
            cinema_id: None, features: vec![], title_substring: None, frequency: "3".into(),
        }]).await.unwrap();

        let users = list_active_users_with_rules(&pool).await.unwrap();
        let ids: Vec<i64> = users.iter().map(|u| u.user_id).collect();
        assert!(ids.contains(&active));
        assert!(!ids.contains(&inactive));
        assert!(!ids.contains(&tg_unverified), "telegram_enabled but unverified must not be active");
        let a = users.iter().find(|u| u.user_id == active).unwrap();
        assert_eq!(a.rules.len(), 1);
        assert_eq!(a.rules[0].frequency, "3");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn load_matchable_showings_joins_cinema_features(pool: PgPool) {
        let mid = crate::db::upsert_movie(&pool, "Cineplexx Linz", "F1", None, &[], None, None)
            .await.unwrap();
        let sid = crate::db::insert_showing(
            &pool, mid, Utc::now() + Duration::days(1), "OV", "Saal 6", "https://x", Utc::now(),
            &["OV".into(), "2D".into()],
        ).await.unwrap().unwrap();
        let ms = load_matchable_showings(&pool, &[sid]).await.unwrap();
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].showing_id, sid);
        assert_eq!(ms[0].title, "F1");
        assert!(ms[0].features.contains(&"OV".to_string()));
    }
```

(Add `use chrono::Duration;` to the test mod imports if not present.)

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test --lib notification::db::tests::list_active_users_with_rules -- --nocapture`
Expected: FAIL (types missing).

- [ ] **Step 3: Implement**

Add to `backend/src/notification/db.rs`:

```rust
use crate::notification::rules::{MatchableShowing, Rule};

#[derive(Debug, Clone)]
pub struct UserRules {
    pub user_id: i64,
    pub email_enabled: bool,
    pub telegram_enabled: bool,
    pub telegram_chat_id: Option<String>,
    pub digest_anchor: DateTime<Utc>,
    pub digest_hour: i32,
    pub rules: Vec<Rule>,
}

pub async fn list_active_users_with_rules(pool: &PgPool) -> sqlx::Result<Vec<UserRules>> {
    // active = email_enabled OR (telegram_enabled AND chat_id not null)
    let prefs: Vec<NotificationPreferences> = sqlx::query_as(
        "SELECT user_id, email_enabled, telegram_enabled, telegram_handle,
                telegram_chat_id, digest_anchor, digest_hour, updated_at
         FROM notification_preferences
         WHERE email_enabled
            OR (telegram_enabled AND telegram_chat_id IS NOT NULL)
         ORDER BY user_id",
    ).fetch_all(pool).await?;

    let mut out = Vec::with_capacity(prefs.len());
    for p in prefs {
        let rules: Vec<NotificationRule> = sqlx::query_as(
            "SELECT id, user_id, position, cinema_id, features, title_substring, frequency
             FROM notification_rule WHERE user_id = $1 ORDER BY position",
        ).bind(p.user_id).fetch_all(pool).await?;
        out.push(UserRules {
            user_id: p.user_id,
            email_enabled: p.email_enabled,
            telegram_enabled: p.telegram_enabled,
            telegram_chat_id: p.telegram_chat_id,
            digest_anchor: p.digest_anchor,
            digest_hour: p.digest_hour,
            rules: rules.into_iter().map(|r| Rule {
                cinema_id: r.cinema_id,
                features: r.features,
                title_substring: r.title_substring,
                frequency: r.frequency,
            }).collect(),
        });
    }
    Ok(out)
}

pub async fn load_matchable_showings(
    pool: &PgPool,
    showing_ids: &[i64],
) -> sqlx::Result<Vec<MatchableShowing>> {
    let rows: Vec<(i64, i64, Vec<String>, String)> = sqlx::query_as(
        "SELECT s.id, m.cinema_id, s.features, m.title
         FROM showing s JOIN movie m ON m.id = s.movie_id
         WHERE s.id = ANY($1)",
    ).bind(showing_ids).fetch_all(pool).await?;
    Ok(rows.into_iter().map(|(showing_id, cinema_id, features, title)| MatchableShowing {
        showing_id, cinema_id, features, title,
    }).collect())
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd backend && cargo test --lib notification::db::tests::list_active_users_with_rules notification::db::tests::load_matchable_showings -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/notification/db.rs
git commit -m "notification/db: list_active_users_with_rules + load_matchable_showings"
```

---

### Task 10: Route showings by rule (batch engine)

**Files:**
- Modify: `backend/src/notification/batch.rs` (`append_showing_for_users` -> `route_showing_for_users`; `handle_batch` passes frequency; rewrite tests)
- Test: `backend/src/notification/batch.rs`

**Interfaces:**
- Produces: `pub async fn route_showing_for_users(pool, showing_id: i64, showing: &MatchableShowing, users: &[UserRules]) -> sqlx::Result<Vec<(i64, String)>>`
- Consumes: `first_match` (Task 5), `UserRules`/`MatchableShowing` (Task 9), `get_or_create_open_batch(pool, user, layer, frequency)` (Task 8).

- [ ] **Step 1: Rewrite the batch tests**

In `backend/src/notification/batch.rs` `mod tests`, update the `prefs_for` helper to the Task-7 shape `(email_enabled, telegram_enabled, handle, digest_anchor, digest_hour)`. Replace `append_showing_for_users(... &[get_preferences(...)])` calls with `route_showing_for_users(... &m, &users)` where `m` is a `MatchableShowing` and `users` is a `Vec<UserRules>`. Add these helpers:

```rust
    fn matchable(showing_id: i64, cinema_id: i64, features: &[&str], title: &str) -> crate::notification::rules::MatchableShowing {
        crate::notification::rules::MatchableShowing {
            showing_id, cinema_id,
            features: features.iter().map(|s| s.to_string()).collect(),
            title: title.to_string(),
        }
    }

    fn user_rules(uid: i64, email: bool, tg: bool, chat: Option<&str>, rules: Vec<crate::notification::rules::Rule>) -> crate::notification::db::UserRules {
        crate::notification::db::UserRules {
            user_id: uid, email_enabled: email, telegram_enabled: tg,
            telegram_chat_id: chat.map(|s| s.to_string()),
            digest_anchor: at(16, 9), digest_hour: 9, rules,
        }
    }

    fn rule(freq: &str) -> crate::notification::rules::Rule {
        crate::notification::rules::Rule {
            cinema_id: None, features: vec![], title_substring: None, frequency: freq.to_string(),
        }
    }
```

Replace the old immediately/3-day/never tests with (full code):

```rust
    #[sqlx::test(migrations = "./migrations")]
    async fn immediate_rule_routes_to_immediate_batch(pool: PgPool) {
        let uid = make_user(&pool, "a@b.com").await;
        prefs_for(&pool, uid, true, false, None, None, None).await;
        let sid = make_showing(&pool, "The Odyssey").await;
        let m = matchable(sid, 1, &["OV"], "The Odyssey");
        let users = vec![user_rules(uid, true, false, None, vec![rule("immediately")])];
        let affected = route_showing_for_users(&pool, sid, &m, &users).await.unwrap();
        assert_eq!(affected, vec![(uid, "email".to_string())]);
        let n: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM notification_batch WHERE user_id=$1 AND layer='email' AND frequency='immediately' AND status='pending'",
        ).bind(uid).fetch_one(&pool).await.unwrap();
        assert_eq!(n.0, 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn never_match_creates_no_batch(pool: PgPool) {
        let uid = make_user(&pool, "c@d.com").await;
        prefs_for(&pool, uid, true, false, None, None, None).await;
        let sid = make_showing(&pool, "F1").await;
        // cinema-specific rule that does not match this showing -> no rule matches -> never
        let m = matchable(sid, 1, &[], "F1");
        let users = vec![user_rules(uid, true, false, None, vec![
            crate::notification::rules::Rule {
                cinema_id: Some(999), features: vec![], title_substring: None,
                frequency: "immediately".to_string(),
            }
        ])];
        let affected = route_showing_for_users(&pool, sid, &m, &users).await.unwrap();
        assert!(affected.is_empty());
        let n: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM notification_batch WHERE user_id=$1 AND status='pending'",
        ).bind(uid).fetch_one(&pool).await.unwrap();
        assert_eq!(n.0, 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn disabled_layer_is_not_routed(pool: PgPool) {
        let uid = make_user(&pool, "e@f.com").await;
        prefs_for(&pool, uid, false, false, None, None, None).await;
        let sid = make_showing(&pool, "F1").await;
        let m = matchable(sid, 1, &["OV"], "F1");
        let users = vec![user_rules(uid, false, false, None, vec![rule("immediately")])];
        let affected = route_showing_for_users(&pool, sid, &m, &users).await.unwrap();
        assert!(affected.is_empty());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn immediate_batch_flushes_same_run(pool: PgPool) {
        let uid = make_user(&pool, "g@h.com").await;
        prefs_for(&pool, uid, true, false, None, None, None).await;
        let sid = make_showing(&pool, "F1").await;
        let m = matchable(sid, 1, &["OV"], "F1");
        let users = vec![user_rules(uid, true, false, None, vec![rule("immediately")])];
        route_showing_for_users(&pool, sid, &m, &users).await.unwrap();
        let email = RecordingEmail::default();
        let sent = process_due_batches(&ctx(&pool, Some(&email), None), at(18, 12)).await.unwrap();
        assert_eq!(sent, 1);
        assert!(email.sent.lock().unwrap()[0].2.contains("F1"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn three_day_batch_not_due_yet(pool: PgPool) {
        let uid = make_user(&pool, "i@j.com").await;
        prefs_for(&pool, uid, true, false, None, Some(at(16, 9)), Some(9)).await;
        let sid = make_showing(&pool, "F1").await;
        let m = matchable(sid, 1, &["OV"], "F1");
        let users = vec![user_rules(uid, true, false, None, vec![rule("3")])];
        route_showing_for_users(&pool, sid, &m, &users).await.unwrap();
        let batch_id = open_batch_id(&pool, uid, "email", "3").await;
        set_batch_created_at(&pool, batch_id, at(19, 8)).await;
        let email = RecordingEmail::default();
        let sent = process_due_batches(&ctx(&pool, Some(&email), None), at(19, 8)).await.unwrap();
        assert_eq!(sent, 0);
        assert_eq!(batch_status(&pool, batch_id).await, "pending");
    }
```

Update `open_batch_id` to take a frequency:

```rust
    async fn open_batch_id(pool: &PgPool, uid: i64, layer: &str, frequency: &str) -> i64 {
        sqlx::query_as::<_, (i64,)>(
            "SELECT id FROM notification_batch WHERE user_id=$1 AND layer=$2 AND frequency=$3 AND status='pending'",
        ).bind(uid).bind(layer).bind(frequency).fetch_one(pool).await.unwrap().0
    }
```

Delete `unverified_telegram_handle_does_not_create_batch` (superseded by `list_active_users_with_rules`). Rewrite `failed_batch_is_retried` to route via a telegram-enabled user with a chat_id (`set_chat_id`) and an `immediately` rule; keep the FlakyTelegram retry assertion. Keep `gc_deletes_failed_batch_older_than_max_retry_age`, but update `create_failed_batch` to call `create_empty_batch(pool, uid, layer, frequency)` and its callers to pass a frequency (e.g. `"immediately"`).

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test --lib notification::batch::tests::immediate_rule_routes_to_immediate_batch -- --nocapture`
Expected: FAIL (`route_showing_for_users` not defined).

- [ ] **Step 3: Implement**

In `backend/src/notification/batch.rs` replace `append_showing_for_users` and add imports:

```rust
use crate::notification::db::{self, DueBatch, UserRules};
use crate::notification::rules::{first_match, MatchableShowing};
```

```rust
pub async fn route_showing_for_users(
    pool: &PgPool,
    showing_id: i64,
    showing: &MatchableShowing,
    users: &[UserRules],
) -> sqlx::Result<Vec<(i64, String)>> {
    let mut affected: Vec<(i64, String)> = Vec::new();
    for u in users {
        let freq = first_match(&u.rules, showing).unwrap_or("never");
        if freq == "never" {
            continue;
        }
        if u.email_enabled {
            let batch_id = db::get_or_create_open_batch(pool, u.user_id, "email", freq).await?;
            db::append_showing_to_batch(pool, batch_id, showing_id).await?;
            affected.push((u.user_id, "email".to_string()));
        }
        if u.telegram_enabled && u.telegram_chat_id.is_some() {
            let batch_id = db::get_or_create_open_batch(pool, u.user_id, "telegram", freq).await?;
            db::append_showing_to_batch(pool, batch_id, showing_id).await?;
            affected.push((u.user_id, "telegram".to_string()));
        }
    }
    Ok(affected)
}
```

In `handle_batch`, the trailing `create_empty_batch` becomes:

```rust
    if let Err(e) = db::create_empty_batch(ctx.pool, batch.user_id, &batch.layer, &batch.frequency).await {
        tracing::warn!(batch_id = batch.batch_id, error = %e, "failed to create next empty batch");
    }
```

Keep `parse_frequency`/`Frequency`/`next_digest_after` (used by `batch_is_due`); drop the `NotificationPreferences` import if now unused.

- [ ] **Step 4: Run to verify pass**

Run: `cd backend && cargo test --lib notification::batch:: -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/notification/batch.rs
git commit -m "notification/batch: route by rules into per-frequency batches"
```

---

### Task 11: Wire routing into the checker

**Files:**
- Modify: `backend/src/checker.rs` (`run_check` step 6)
- Test: `backend/src/checker.rs`

**Interfaces:**
- Consumes: `list_active_users_with_rules`, `load_matchable_showings`, `route_showing_for_users`.

- [ ] **Step 1: Write the failing test**

Add to `backend/src/checker.rs` `mod tests`:

```rust
    #[sqlx::test(migrations = "./migrations")]
    async fn new_showing_routes_by_user_rules(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(dir.path(), false);
        let http = http();
        let fetcher = FakeFetcher { result: Ok((vec![make_showing(20)], HashMap::new())) };
        let uid = crate::db::find_or_create_user(&pool, "email", "a@b.com", "a@b.com").await.unwrap();
        crate::notification::db::upsert_preferences(&pool, uid, crate::notification::db::PreferenceUpdate {
            email_enabled: Some(true), ..Default::default()
        }).await.unwrap();
        crate::notification::db::replace_rules(&pool, uid, &[
            crate::notification::db::RuleInput {
                cinema_id: None, features: vec![], title_substring: None, frequency: "immediately".into(),
            },
        ]).await.unwrap();
        let email = RecordingEmail::default();
        let c = CheckCtx {
            pool: &pool, http: &http, config: &cfg, notifier: None,
            fetchers: vec![("cineplexx", &fetcher)], email: Some(&email), telegram: None,
        };
        let r = run_check(&c, now()).await.unwrap();
        assert_eq!((r.new_showings, r.total_showings), (1, 1));
        assert_eq!(email.sent.lock().unwrap().len(), 1, "immediate catch-all rule flushes immediately");
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test --lib checker::tests::new_showing_routes_by_user_rules -- --nocapture`
Expected: FAIL (checker still calls `append_showing_for_users`).

- [ ] **Step 3: Implement**

In `backend/src/checker.rs` `run_check`, replace step 6 (the `if !new_showing_ids.is_empty() { ... append_showing_for_users ... }` block) with:

```rust
    if !new_showing_ids.is_empty() {
        let users = crate::notification::db::list_active_users_with_rules(ctx.pool).await?;
        if !users.is_empty() {
            let ids: Vec<i64> = new_showing_ids.iter().map(|(id, _)| *id).collect();
            let matchable = crate::notification::db::load_matchable_showings(ctx.pool, &ids).await?;
            for m in &matchable {
                crate::notification::batch::route_showing_for_users(ctx.pool, m.showing_id, m, &users).await?;
            }
        }
    }
```

The `process_due_batches` call right after stays unchanged.

- [ ] **Step 4: Run to verify pass**

Run: `cd backend && cargo test --lib checker:: -- --nocapture`
Expected: PASS. (The existing `new_showing_creates_batch_for_immediate_user` test references `email_frequency`; update its `upsert_preferences` to `email_enabled: Some(true)` and add a catch-all `replace_rules`. If hard to adapt, replace it with the new test above and delete the old one.)

- [ ] **Step 5: Commit**

```bash
git add backend/src/checker.rs
git commit -m "checker: route new showings through user rules"
```

---

### Task 12: Preferences API reshape + Rules API

**Files:**
- Modify: `backend/src/notification/api.rs`, `backend/src/notification/mod.rs`, `backend/src/web.rs`
- Test: `backend/src/notification/api.rs`

**Interfaces:**
- Produces (camelCase JSON):
  - `GET /api/preferences` -> `{ emailEnabled, telegramEnabled, telegramHandle?, telegramVerified, digestAnchor, digestHour }`
  - `PUT /api/preferences` -> same fields.
  - `GET /api/preferences/rules` -> `{ rules: [{id, position, cinemaId?, cinemaName?, features[], titleSubstring?, frequency}], cinemas: [{id, name}] }`
  - `PUT /api/preferences/rules` -> full ordered array; returns same shape.
- Validation: frequency in vocab; features subset of vocabulary; titleSubstring <= 200; <= 32 rules.

- [ ] **Step 1: Write the failing tests**

Add to `backend/src/notification/api.rs` `mod tests`. Rewrite the existing prefs tests that reference `emailFrequency`/`telegramFrequency` to use `emailEnabled`/`telegramEnabled` (checkboxes). Add rule tests:

```rust
    async fn seed_rules_user(pool: &PgPool) -> i64 {
        let uid = crate::db::find_or_create_user(pool, "email", "rules@api.com", "rules@api.com").await.unwrap();
        crate::notification::db::upsert_preferences(pool, uid, crate::notification::db::PreferenceUpdate {
            email_enabled: Some(true), ..Default::default()
        }).await.unwrap();
        uid
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_rules_returns_empty_plus_cinemas(pool: PgPool) {
        let uid = seed_rules_user(&pool).await;
        let token = make_session(&pool, uid).await;
        let app = crate::web::router(test_state(pool));
        let resp = app.oneshot(
            Request::get("/api/preferences/rules").header("Cookie", format!("ov_session={token}"))
                .body(axum::body::Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(resp.status(), 200);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["rules"], serde_json::json!([]));
        let names: Vec<String> = json["cinemas"].as_array().unwrap().iter()
            .map(|c| c["name"].as_str().unwrap().to_string()).collect();
        assert!(names.contains(&"Cineplexx Linz".to_string()));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn put_rules_replaces_and_rolls_over(pool: PgPool) {
        let uid = seed_rules_user(&pool).await;
        crate::notification::db::get_or_create_open_batch(&pool, uid, "email", "immediately").await.unwrap();
        let token = make_session(&pool, uid).await;
        let app = crate::web::router(test_state(pool.clone()));
        let body = r#"{"rules":[{"cinemaId":1,"features":["IMAX","Atmos"],"titleSubstring":null,"frequency":"immediately"},{"cinemaId":null,"features":[],"titleSubstring":null,"frequency":"3"}]}"#;
        let resp = app.oneshot(
            Request::put("/api/preferences/rules").header("Cookie", format!("ov_session={token}"))
                .header("Content-Type", "application/json").body(axum::body::Body::from(body)).unwrap()
        ).await.unwrap();
        assert_eq!(resp.status(), 200);
        let json: serde_json::Value = serde_json::from_slice(&to_bytes(resp.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(json["rules"].as_array().unwrap().len(), 2);
        assert_eq!(json["rules"][0]["frequency"], "immediately");
        assert_eq!(json["rules"][1]["frequency"], "3");
        let n: (i64,) = sqlx::query_as("SELECT count(*) FROM notification_batch WHERE user_id=$1 AND status='pending'")
            .bind(uid).fetch_one(&pool).await.unwrap();
        assert_eq!(n.0, 0, "open batches rolled over on save");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn put_rules_rejects_bad_frequency(pool: PgPool) {
        let uid = seed_rules_user(&pool).await;
        let token = make_session(&pool, uid).await;
        let app = crate::web::router(test_state(pool));
        let resp = app.oneshot(
            Request::put("/api/preferences/rules").header("Cookie", format!("ov_session={token}"))
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(r#"{"rules":[{"cinemaId":null,"features":[],"titleSubstring":null,"frequency":"sometimes"}]}"#)).unwrap()
        ).await.unwrap();
        assert_eq!(resp.status(), 400);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_rules_unauthenticated_401(pool: PgPool) {
        let app = crate::web::router(test_state(pool));
        let resp = app.oneshot(
            Request::get("/api/preferences/rules").body(axum::body::Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(resp.status(), 401);
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test --lib notification::api::tests::get_rules_returns_empty_plus_cinemas -- --nocapture`
Expected: FAIL (routes missing).

- [ ] **Step 3: Implement preferences reshape**

In `backend/src/notification/api.rs`:

```rust
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceUpdateRequest {
    pub email_enabled: Option<bool>,
    pub telegram_enabled: Option<bool>,
    pub telegram_handle: Option<String>,
    pub digest_anchor: Option<DateTime<Utc>>,
    pub digest_hour: Option<i32>,
}

impl From<PreferenceUpdateRequest> for crate::notification::db::PreferenceUpdate {
    fn from(req: PreferenceUpdateRequest) -> Self {
        Self {
            email_enabled: req.email_enabled,
            telegram_enabled: req.telegram_enabled,
            telegram_handle: req.telegram_handle,
            digest_anchor: req.digest_anchor,
            digest_hour: req.digest_hour,
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesResponse {
    pub email_enabled: bool,
    pub telegram_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telegram_handle: Option<String>,
    pub telegram_verified: bool,
    pub digest_anchor: DateTime<Utc>,
    pub digest_hour: i32,
}

impl From<crate::notification::db::NotificationPreferences> for PreferencesResponse {
    fn from(p: crate::notification::db::NotificationPreferences) -> Self {
        PreferencesResponse {
            email_enabled: p.email_enabled,
            telegram_enabled: p.telegram_enabled,
            telegram_handle: p.telegram_handle,
            telegram_verified: p.telegram_chat_id.is_some(),
            digest_anchor: p.digest_anchor,
            digest_hour: p.digest_hour,
        }
    }
}
```

In `put_preferences`: keep `changed_digest`; set `rollover_email = dto.email_enabled.is_some() || changed_digest;` and `rollover_telegram = dto.telegram_enabled.is_some() || changed_digest;`. In `delete_telegram`: set `telegram_enabled: Some(false)` and `telegram_handle: Some(String::new())`. In `validate_update`: drop `is_valid_frequency`; keep `digest_hour` range check.

- [ ] **Step 4: Implement rules routes**

Add to `backend/src/notification/api.rs`:

```rust
const FEATURE_VOCABULARY: &[&str] = &["OV","OmU","OmdU","2D","3D","IMAX","Atmos","DolbyCinema","4DX"];
const MAX_RULES: usize = 32;
const MAX_TITLE_LEN: usize = 200;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleRequest {
    pub cinema_id: Option<i64>,
    pub features: Vec<String>,
    pub title_substring: Option<String>,
    pub frequency: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleResponse {
    pub id: i64,
    pub position: i32,
    pub cinema_id: Option<i64>,
    pub cinema_name: Option<String>,
    pub features: Vec<String>,
    pub title_substring: Option<String>,
    pub frequency: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RulesResponse {
    pub rules: Vec<RuleResponse>,
    pub cinemas: Vec<CinemaDto>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CinemaDto { pub id: i64, pub name: String }

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RulesPutRequest { pub rules: Vec<RuleRequest> }

pub fn rules_router() -> Router<AppState> {
    Router::new()
        .route("/api/preferences/rules", routing::get(get_rules))
        .route("/api/preferences/rules", routing::put(put_rules))
}

async fn get_rules(State(state): State<AppState>, auth: AuthUser) -> Result<Json<RulesResponse>, StatusCode> {
    let rules = crate::notification::db::list_rules(&state.pool, auth.user_id).await
        .map_err(|e| { tracing::error!("list_rules failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR })?;
    let cinemas = crate::notification::db::list_cinemas(&state.pool).await
        .map_err(|e| { tracing::error!("list_cinemas failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR })?;
    let cinema_map: std::collections::HashMap<i64, String> = cinemas.iter().cloned().collect();
    let rules = rules.into_iter().map(|r| RuleResponse {
        id: r.id, position: r.position,
        cinema_name: r.cinema_id.and_then(|id| cinema_map.get(&id).cloned()),
        cinema_id: r.cinema_id, features: r.features, title_substring: r.title_substring,
        frequency: r.frequency,
    }).collect();
    let cinemas = cinemas.into_iter().map(|(id, name)| CinemaDto { id, name }).collect();
    Ok(Json(RulesResponse { rules, cinemas }))
}

async fn put_rules(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<RulesPutRequest>,
) -> Result<Json<RulesResponse>, StatusCode> {
    validate_rules(&body.rules)?;
    let input: Vec<crate::notification::db::RuleInput> = body.rules.into_iter().map(|r|
        crate::notification::db::RuleInput {
            cinema_id: r.cinema_id, features: r.features,
            title_substring: r.title_substring, frequency: r.frequency,
        }).collect();
    crate::notification::db::replace_rules(&state.pool, auth.user_id, &input).await
        .map_err(|e| { tracing::error!("replace_rules failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR })?;
    for layer in ["email", "telegram"] {
        let _ = crate::notification::db::delete_open_batch(&state.pool, auth.user_id, layer).await;
    }
    get_rules(State(state), auth).await
}

fn validate_rules(rules: &[RuleRequest]) -> Result<(), StatusCode> {
    if rules.len() > MAX_RULES { return Err(StatusCode::BAD_REQUEST); }
    let is_freq = |f: &str| f == "never" || f == "immediately" || matches!(f.parse::<i32>(), Ok(d) if (1..=7).contains(&d));
    for r in rules {
        if !is_freq(&r.frequency) { return Err(StatusCode::BAD_REQUEST); }
        if let Some(t) = &r.title_substring {
            if t.chars().count() > MAX_TITLE_LEN { return Err(StatusCode::BAD_REQUEST); }
        }
        for f in &r.features {
            if !FEATURE_VOCABULARY.contains(&f.as_str()) { return Err(StatusCode::BAD_REQUEST); }
        }
    }
    Ok(())
}
```

In `backend/src/notification/mod.rs` add `pub use api::rules_router;`. In `backend/src/web.rs` add `.merge(crate::notification::rules_router())` next to `preferences_router()`.

- [ ] **Step 5: Run to verify pass**

Run: `cd backend && cargo test --lib notification::api:: -- --nocapture`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add backend/src/notification/api.rs backend/src/notification/mod.rs backend/src/web.rs
git commit -m "notification/api: enablement prefs + rules CRUD routes"
```

---

### Task 13: Frontend - types, API, rule editor

**Files:**
- Modify: `frontend/src/types.ts`, `frontend/src/api/preferences.ts`, `frontend/src/pages/PreferencesPage.tsx`, `frontend/src/pages/PreferencesPage.test.tsx`, `frontend/src/locales/{en,de}.json`
- Test: Vitest (`frontend/src/pages/PreferencesPage.test.tsx`)

**Interfaces:**
- Produces: `NotificationPreferences` with `emailEnabled`/`telegramEnabled`; `NotificationRule`, `Cinema`, `RulesResponse`, `FEATURES`; `fetchRules`/`saveRules`; a compact-row rule editor on `PreferencesPage`.

- [ ] **Step 1: Write the failing tests**

Rewrite `frontend/src/pages/PreferencesPage.test.tsx`. Update `mockPrefs` and `mockFetch`:

```ts
const mockPrefs: NotificationPreferences = {
  emailEnabled: true,
  telegramEnabled: false,
  telegramHandle: "",
  telegramVerified: false,
  digestAnchor: "2026-08-09T09:00:00+02:00",
  digestHour: 9,
};

function mockFetch(prefs: NotificationPreferences | Error) {
  vi.stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    if (url.startsWith("/api/auth/me")) return { ok: true, json: async () => ({ id: 1, email: "a@b.c" }) };
    if (url.startsWith("/api/auth/providers")) return { ok: true, json: async () => ({ email: true, google: true, github: true, dev: false }) };
    if (url.startsWith("/api/preferences/rules")) {
      if (init && init.method === "PUT") return { ok: true, json: async () => JSON.parse(String(init.body)) };
      return { ok: true, json: async () => ({ rules: [], cinemas: [{ id: 1, name: "Cineplexx Linz" }, { id: 2, name: "Megaplex PlusCity" }] }) };
    }
    if (url.startsWith("/api/preferences")) {
      if (init && init.method === "PUT") return { ok: true, json: async () => JSON.parse(String(init.body)) };
      if (prefs instanceof Error) return { ok: false, status: 500 };
      return { ok: true, json: async () => prefs };
    }
    return { ok: false, status: 404 };
  }));
}
```

Replace the frequency-select assertions with enable-checkbox assertions:

```ts
  it("renders both channels with enable toggles from the fetched preferences", async () => {
    mockFetch(mockPrefs);
    renderPage();
    await screen.findByRole("heading", { name: "Notification preferences" });
    expect(screen.getByLabelText("Email")).toBeChecked();
    expect(screen.getByLabelText("Telegram")).not.toBeChecked();
  });
```

Add a rule-editor test:

```ts
  it("adds a rule, sets frequency, and saves the ordered list", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.startsWith("/api/auth/me")) return { ok: true, json: async () => ({ id: 1, email: "a@b.c" }) };
      if (url.startsWith("/api/auth/providers")) return { ok: true, json: async () => ({ email: true, google: true, github: true, dev: false }) };
      if (url.startsWith("/api/preferences/rules")) {
        if (init && init.method === "PUT") return { ok: true, json: async () => JSON.parse(String(init.body)) };
        return { ok: true, json: async () => ({ rules: [], cinemas: [{ id: 1, name: "Cineplexx Linz" }, { id: 2, name: "Megaplex PlusCity" }] }) };
      }
      if (url.startsWith("/api/preferences")) {
        if (init && init.method === "PUT") return { ok: true, json: async () => JSON.parse(String(init.body)) };
        return { ok: true, json: async () => mockPrefs };
      }
      return { ok: false, status: 404 };
    });
    vi.stubGlobal("fetch", fetchMock);
    renderPage();
    await screen.findByRole("heading", { name: "Notification preferences" });
    fireEvent.click(screen.getByRole("button", { name: "Add rule" }));
    const freq = await screen.findByLabelText("Rule 1 frequency");
    fireEvent.change(freq, { target: { value: "immediately" } });
    fireEvent.click(screen.getByRole("button", { name: "Save rules" }));
    await waitFor(() => {
      const put = fetchMock.mock.calls.find(([u, i]) => String(u).startsWith("/api/preferences/rules") && i && i.method === "PUT");
      expect(put).toBeDefined();
      const body = JSON.parse(String(put![1]!.body));
      expect(body.rules[0].frequency).toBe("immediately");
    });
  });
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && npm test -- --run PreferencesPage`
Expected: FAIL (types/components not updated).

- [ ] **Step 3: Implement types + API**

`frontend/src/types.ts` (replace the `NotificationPreferences` block and add new types; keep `NotificationFrequency`/`FREQUENCY_OPTIONS`):

```ts
export interface NotificationPreferences {
  emailEnabled: boolean;
  telegramEnabled: boolean;
  telegramHandle: string;
  telegramVerified: boolean;
  digestAnchor: string;
  digestHour: number;
}

export const FEATURES = ["OV", "OmU", "OmdU", "2D", "3D", "IMAX", "Atmos", "DolbyCinema", "4DX"] as const;
export type Feature = (typeof FEATURES)[number];

export interface Cinema { id: number; name: string; }

export interface NotificationRule {
  id?: number;
  position: number;
  cinemaId: number | null;
  cinemaName?: string | null;
  features: string[];
  titleSubstring: string | null;
  frequency: NotificationFrequency;
}

export interface RulesResponse { rules: NotificationRule[]; cinemas: Cinema[]; }
```

`frontend/src/api/preferences.ts`:

```ts
import type { NotificationPreferences, RulesResponse, NotificationRule } from "../types";

export async function fetchPreferences(): Promise<NotificationPreferences> {
  const res = await fetch("/api/preferences", { credentials: "include" });
  if (!res.ok) throw new Error("failed to load preferences");
  return res.json();
}

export async function savePreferences(prefs: Partial<NotificationPreferences>): Promise<NotificationPreferences> {
  const res = await fetch("/api/preferences", {
    method: "PUT", headers: { "Content-Type": "application/json" },
    credentials: "include", body: JSON.stringify(prefs),
  });
  if (!res.ok) throw new Error("failed to save preferences");
  return res.json();
}

export async function fetchRules(): Promise<RulesResponse> {
  const res = await fetch("/api/preferences/rules", { credentials: "include" });
  if (!res.ok) throw new Error("failed to load rules");
  return res.json();
}

export async function saveRules(rules: NotificationRule[]): Promise<RulesResponse> {
  const res = await fetch("/api/preferences/rules", {
    method: "PUT", headers: { "Content-Type": "application/json" },
    credentials: "include",
    body: JSON.stringify({ rules: rules.map((r) => ({ cinemaId: r.cinemaId, features: r.features, titleSubstring: r.titleSubstring, frequency: r.frequency })) }),
  });
  if (!res.ok) throw new Error("failed to save rules");
  return res.json();
}
```

- [ ] **Step 4: Implement the page**

In `frontend/src/pages/PreferencesPage.tsx`, replace each per-channel frequency `<select>` with an enable checkbox. Build `channels` as:

```tsx
  const channels: Array<{ name: "email" | "telegram"; enabled: boolean; onToggle: (v: boolean) => void }> = [
    { name: "email", enabled: prefs.emailEnabled, onChange: (v) => setPrefs({ ...prefs, emailEnabled: v }) },
    { name: "telegram", enabled: prefs.telegramEnabled, onChange: (v) => setPrefs({ ...prefs, telegramEnabled: v }) },
  ];
```

and render:

```tsx
          <label className="pref-field">
            <span>{t("preferences." + c.name)}</span>
            <input
              type="checkbox"
              aria-label={t("preferences." + c.name)}
              checked={c.enabled}
              onChange={(e) => c.onChange(e.target.checked)}
            />
          </label>
```

Below the channel cards, add a rule editor. Fetch rules + cinemas alongside preferences in the `useEffect` (call `fetchRules()` too). Add state `rules`/`cinemas` and an `updateRule(i, patch)` helper. Render each rule as a card (compact row: cinema select, title input, feature toggle chips, frequency select, delete button) with an "Add rule" button and a "Save rules" button (calls `saveRules(rules)`).

A minimal rule-row render (no drag for v1; ordering fixed by array order, delete + add):

```tsx
      <h3>{t("preferences.rulesTitle")}</h3>
      <p className="pref-desc">{t("preferences.rulesDesc")}</p>
      {rules.map((r, i) => (
        <div className="card pref-card" key={i}>
          <div className="rule-row">
            <select aria-label={"Rule " + (i + 1) + " cinema"} value={r.cinemaId ?? ""} onChange={(e) => updateRule(i, { cinemaId: e.target.value ? Number(e.target.value) : null })}>
              <option value="">{t("preferences.anyCinema")}</option>
              {cinemas.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
            </select>
            <input aria-label={"Rule " + (i + 1) + " title"} placeholder={t("preferences.anyTitle")} value={r.titleSubstring ?? ""} onChange={(e) => updateRule(i, { titleSubstring: e.target.value || null })} />
            <select aria-label={"Rule " + (i + 1) + " frequency"} value={r.frequency} onChange={(e) => updateRule(i, { frequency: e.target.value as NotificationFrequency })}>
              {FREQUENCY_OPTIONS.map((v) => <option key={v} value={v}>{frequencyLabel(t, v)}</option>)}
            </select>
            <button className="mock-button" onClick={() => removeRule(i)}>{"x"}</button>
          </div>
          <div className="rule-features">
            {FEATURES.map((f) => (
              <button key={f} className={"chip " + (r.features.includes(f) ? "chip-on" : "")} onClick={() => toggleFeature(i, f)}>{f}</button>
            ))}
          </div>
        </div>
      ))}
      <button className="auth-submit" onClick={addRule}>{t("preferences.addRule")}</button>
      <button className="auth-submit" onClick={handleSaveRules}>{t("preferences.saveRules")}</button>
      {rulesSaved && <span className="pref-saved">{t("preferences.saved")}</span>}
```

Helpers:

```tsx
  const addRule = () => setRules([...rules, { position: rules.length, cinemaId: null, features: [], titleSubstring: null, frequency: "3" }]);
  const removeRule = (i: number) => setRules(rules.filter((_, idx) => idx !== i).map((r, idx) => ({ ...r, position: idx })));
  const updateRule = (i: number, patch: Partial<NotificationRule>) => setRules(rules.map((r, idx) => idx === i ? { ...r, ...patch } : r));
  const toggleFeature = (i: number, f: string) => setRules(rules.map((r, idx) => idx === i ? { ...r, features: r.features.includes(f) ? r.features.filter((x) => x !== f) : [...r.features, f] } : r));
  const handleSaveRules = async () => { const res = await saveRules(rules); setRules(res.rules); setRulesSaved(true); };
```

- [ ] **Step 5: Add i18n keys**

In `frontend/src/locales/en.json` under `preferences`, add:

```json
    "rulesTitle": "Rules",
    "rulesDesc": "First matching rule wins. Add a catch-all (any cinema, any feature) as the last rule.",
    "anyCinema": "Any cinema",
    "anyTitle": "any title",
    "addRule": "Add rule",
    "saveRules": "Save rules"
```

In `frontend/src/locales/de.json` under `preferences`, add:

```json
    "rulesTitle": "Regeln",
    "rulesDesc": "Die erste passende Regel gewinnt. Leg als letzte Regel eine Auffangregel (beliebiges Kino, beliebiges Feature) an.",
    "anyCinema": "Beliebiges Kino",
    "anyTitle": "beliebiger Titel",
    "addRule": "Regel hinzufügen",
    "saveRules": "Regeln speichern"
```

- [ ] **Step 6: Run to verify pass**

Run: `cd frontend && npm test -- --run PreferencesPage`
Expected: PASS.

- [ ] **Step 7: Run full verification**

Run: `cd backend && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`
Run: `cd frontend && npm test && npm run build`
Expected: all green. (Backend tests need `docker compose up -d db` + `DATABASE_URL`.)

- [ ] **Step 8: Commit**

```bash
git add frontend/src frontend/src/locales
git commit -m "frontend: notification rule editor + enablement prefs"
```
