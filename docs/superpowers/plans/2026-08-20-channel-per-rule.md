# Channel-per-rule notifications — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the notification channel choice (Email / Telegram / Both) from global per-user toggles onto each notification rule, drop the global enablement columns, and fix the unstyled rule-card CSS.

**Architecture:** Add a `channels TEXT[]` column to `notification_rule` (subset of `{email, telegram}`), backfill from the old global enablement, then drop the global `email_enabled`/`telegram_enabled` from `notification_preferences`. Routing reads `channels` from the first matching rule. Frontend gains a 3-way `<select>` per rule; the global toggle cards disappear; the Telegram handle + verification UI stays as a compact card.

**Tech Stack:** Rust (axum + sqlx/Postgres), React 19 + Vite + TypeScript, vitest. Tests: `cargo test` (sqlx::test, per-test DB via `docker compose up -d db`), `npm test` + `npm run build`.

**Spec:** `docs/superpowers/specs/2026-08-20-channel-per-rule-design.md`

## Global Constraints

- Migration files live in `backend/migrations/`, numbered `0007_channel_per_rule.sql` and `0008_drop_global_enablement.sql`. Never edit an existing migration.
- Rust structs that mirror DB rows use `sqlx::FromRow` and `#[serde(rename_all = "camelCase")]` where they're API DTOs (see `api.rs`).
- Channel vocabulary is exactly `{"email", "telegram"}`. The frontend uses a single `channel: "email" | "telegram" | "both"` and converts to/from the array at the API boundary.
- Keep the build green between tasks: additive changes first, drop columns last.
- No emojis in code or copy. German + English copy in `locales/de.json` and `locales/en.json`.
- Verification per backend task: `cd backend && cargo test` (needs `DATABASE_URL` + `docker compose up -d db`). Verification per frontend task: `cd frontend && npm test && npm run build`.

## File Structure

- `backend/migrations/0007_channel_per_rule.sql` — NEW. Adds `channels TEXT[]` to `notification_rule`, backfills from enablement.
- `backend/migrations/0008_drop_global_enablement.sql` — NEW. Drops `email_enabled`/`telegram_enabled` from `notification_preferences`.
- `backend/src/notification/rules.rs` — `Rule` gains `channels`; `first_match` returns `Option<&Rule>`.
- `backend/src/notification/db.rs` — `RuleInput`/`NotificationRule` gain `channels`; `UserRules`/`NotificationPreferences`/`PreferenceUpdate` lose enablement fields in Task 3; `list_active_users_with_rules` query rewritten in Task 3.
- `backend/src/notification/batch.rs` — `route_showing_for_users` gates on `rule.channels`; test helpers updated.
- `backend/src/notification/api.rs` — `RuleRequest`/`RuleResponse` gain `channels`; `validate_rules` rejects bad channels; `PreferenceUpdateRequest`/`PreferencesResponse`/`PreferenceUpdate` lose enablement fields in Task 3.
- `backend/src/notification/verify.rs` — test-only `PreferenceUpdate` usage updated in Task 3.
- `backend/src/checker.rs` — test-only `RuleInput` (Task 1) and `PreferenceUpdate` (Task 3) usages updated.
- `frontend/src/types.ts` — `NotificationPreferences` drops enablement; `NotificationRule` gains `channel`.
- `frontend/src/api/preferences.ts` — `savePreferences` body trimmed; `saveRules`/`fetchRules` convert `channel ↔ channels[]`.
- `frontend/src/pages/PreferencesPage.tsx` — remove toggle cards; add Telegram-Konto compact card; add channel `<select>`; default `channel: "both"`; `.rule-warn` badge; wrap buttons in `.pref-actions`.
- `frontend/src/index.css` — add `.rule-row`, `.rule-features`, `.chip`, `.chip-on`, `.rule-remove`, `.rule-warn`, responsive rules.
- `frontend/src/locales/{en,de}.json` — channel labels, telegram-unverified warning, softened telegramDesc.
- `frontend/src/pages/PreferencesPage.test.tsx` — drop enablement from `mockPrefs`; replace toggle test; add channel-select + save test.

---

### Task 1: Add `channels` field to rules (additive)

Additive-only: introduces the `channels` column and field everywhere, keeps old enablement fields and routing. Build stays green. `first_match` is refactored to return `&Rule` (mechanical; caller still reads `.frequency`).

**Files:**
- Create: `backend/migrations/0007_channel_per_rule.sql`
- Modify: `backend/src/notification/rules.rs`
- Modify: `backend/src/notification/db.rs`
- Modify: `backend/src/notification/batch.rs`
- Modify: `backend/src/checker.rs` (tests only)

**Interfaces:**
- Produces: `Rule { cinema_id, features, title_substring, frequency, channels: Vec<String> }`; `first_match(rules, showing) -> Option<&Rule>`; `RuleInput { ..., channels: Vec<String> }`; `NotificationRule { ..., channels: Vec<String> }`. `route_showing_for_users` still gates on `u.email_enabled`/`u.telegram_enabled` (unchanged semantics) — Task 2 changes that.

- [ ] **Step 1: Write the failing db test for channels round-trip**

Add to `backend/src/notification/db.rs` `#[cfg(test)] mod tests`, after the existing `replace_rules_is_replacement` test:

```rust
    #[sqlx::test(migrations = "./migrations")]
    async fn replace_rules_round_trips_channels(pool: PgPool) {
        let uid = make_rule_user(&pool).await;
        let inserted = replace_rules(
            &pool,
            uid,
            &[
                RuleInput {
                    cinema_id: None,
                    features: vec![],
                    title_substring: None,
                    frequency: "3".into(),
                    channels: vec!["email".into(), "telegram".into()],
                },
                RuleInput {
                    cinema_id: None,
                    features: vec![],
                    title_substring: None,
                    frequency: "immediately".into(),
                    channels: vec!["telegram".into()],
                },
            ],
        )
        .await
        .unwrap();
        assert_eq!(inserted[0].channels, vec!["email", "telegram"]);
        assert_eq!(inserted[1].channels, vec!["telegram"]);
        let listed = list_rules(&pool, uid).await.unwrap();
        assert_eq!(listed[0].channels, vec!["email", "telegram"]);
        assert_eq!(listed[1].channels, vec!["telegram"]);
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd backend && cargo test --lib replace_rules_round_trips_channels`
Expected: compile error — `RuleInput` has no field `channels`.

- [ ] **Step 3: Create the migration**

Create `backend/migrations/0007_channel_per_rule.sql`:

```sql
-- 1. Per-rule channel array (default email so existing rows are valid)
ALTER TABLE notification_rule
  ADD COLUMN channels TEXT[] NOT NULL DEFAULT '{email}';

-- 2. Backfill each rule's channels from its user's old enablement
UPDATE notification_rule r
SET channels = CASE
  WHEN p.email_enabled AND p.telegram_enabled THEN ARRAY['email','telegram']::text[]
  WHEN p.email_enabled                         THEN ARRAY['email']::text[]
  WHEN p.telegram_enabled                      THEN ARRAY['telegram']::text[]
  ELSE                                              ARRAY['email']::text[]
END
FROM notification_preferences p
WHERE p.user_id = r.user_id;
```

- [ ] **Step 4: Add `channels` to the Rust rule structs**

In `backend/src/notification/rules.rs`, add the field to `Rule`:

```rust
#[derive(Debug, Clone)]
pub struct Rule {
    pub cinema_id: Option<i64>,
    pub features: Vec<String>,
    pub title_substring: Option<String>,
    pub frequency: String,
    pub channels: Vec<String>,
}
```

Update the `rule()` test helper in the same file's `mod tests` to include `channels`:

```rust
    fn rule(cinema_id: Option<i64>, features: &[&str], title: Option<&str>, freq: &str) -> Rule {
        Rule {
            cinema_id,
            features: features.iter().map(|s| s.to_string()).collect(),
            title_substring: title.map(|s| s.to_string()),
            frequency: freq.to_string(),
            channels: vec!["email".into()],
        }
    }
```

In `backend/src/notification/db.rs`, add `channels` to `RuleInput` and `NotificationRule`:

```rust
#[derive(Debug, Clone)]
pub struct RuleInput {
    pub cinema_id: Option<i64>,
    pub features: Vec<String>,
    pub title_substring: Option<String>,
    pub frequency: String,
    pub channels: Vec<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct NotificationRule {
    pub id: i64,
    #[allow(dead_code)]
    pub user_id: i64,
    pub position: i32,
    pub cinema_id: Option<i64>,
    pub features: Vec<String>,
    pub title_substring: Option<String>,
    pub frequency: String,
    pub channels: Vec<String>,
}
```

Update `list_rules` and `replace_rules` queries (both the INSERT and the SELECTs) to include `channels`:

```rust
pub async fn list_rules(pool: &PgPool, user_id: i64) -> sqlx::Result<Vec<NotificationRule>> {
    sqlx::query_as(
        "SELECT id, user_id, position, cinema_id, features, title_substring, frequency, channels
         FROM notification_rule WHERE user_id = $1 ORDER BY position",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn replace_rules(
    pool: &PgPool,
    user_id: i64,
    input: &[RuleInput],
) -> sqlx::Result<Vec<NotificationRule>> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM notification_rule WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    for (i, r) in input.iter().enumerate() {
        sqlx::query(
            "INSERT INTO notification_rule
               (user_id, position, cinema_id, features, title_substring, frequency, channels)
             VALUES ($1, $2, $3, $4, NULLIF($5, ''), $6, $7)",
        )
        .bind(user_id)
        .bind(i as i32)
        .bind(r.cinema_id)
        .bind(&r.features)
        .bind(r.title_substring.as_deref())
        .bind(&r.frequency)
        .bind(&r.channels)
        .execute(&mut *tx)
        .await?;
    }
    let rows = sqlx::query_as::<_, NotificationRule>(
        "SELECT id, user_id, position, cinema_id, features, title_substring, frequency, channels
         FROM notification_rule WHERE user_id = $1 ORDER BY position",
    )
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows)
}
```

Update the `NotificationRule → Rule` mapping inside `list_active_users_with_rules` (the `.map(|r| Rule { ... })` closure) to pass `channels: r.channels`.

Update the inner `list_rules`-duplicate SELECT inside `list_active_users_with_rules` to also select `channels`.

- [ ] **Step 5: Update db.rs test helpers that build `RuleInput`**

In `backend/src/notification/db.rs` `mod tests`, update the `input()` helper:

```rust
    fn input(
        cinema_id: Option<i64>,
        features: &[&str],
        title: Option<&str>,
        freq: &str,
    ) -> RuleInput {
        RuleInput {
            cinema_id,
            features: features.iter().map(|s| s.to_string()).collect(),
            title_substring: title.map(|s| s.to_string()),
            frequency: freq.to_string(),
            channels: vec!["email".into()],
        }
    }
```

Update the `list_active_users_with_rules_filters_inactive` test's inline `RuleInput` (currently `frequency: "3".into()`) to add `channels: vec![]` — wait, empty channels is rejected by routing later; for this test use `channels: vec!["email".into()]`:

```rust
        replace_rules(
            &pool,
            active,
            &[RuleInput {
                cinema_id: None,
                features: vec![],
                title_substring: None,
                frequency: "3".into(),
                channels: vec!["email".into()],
            }],
        )
        .await
        .unwrap();
```

- [ ] **Step 6: Refactor `first_match` to return `Option<&Rule>`**

In `backend/src/notification/rules.rs`:

```rust
pub fn first_match<'a>(rules: &'a [Rule], s: &MatchableShowing) -> Option<&'a Rule> {
    for r in rules {
        if matches(r, s) {
            return Some(r);
        }
    }
    None
}
```

Update the `first_match` tests in `rules.rs` `mod tests` to assert on `.frequency`:

```rust
    #[test]
    fn first_match_wins_in_order() {
        let rules = vec![
            rule(Some(7), &["IMAX"], None, "immediately"),
            rule(None, &[], None, "3"),
        ];
        assert_eq!(
            first_match(&rules, &showing(7, &["IMAX"], "X")).map(|r| r.frequency.as_str()),
            Some("immediately")
        );
        assert_eq!(
            first_match(&rules, &showing(8, &["OV"], "Y")).map(|r| r.frequency.as_str()),
            Some("3")
        );
    }

    #[test]
    fn no_match_returns_none() {
        let rules = vec![rule(Some(7), &[], None, "immediately")];
        assert_eq!(first_match(&rules, &showing(9, &[], "X")), None);
    }
```

- [ ] **Step 7: Update `route_showing_for_users` caller + batch.rs test helpers**

In `backend/src/notification/batch.rs`, update the caller (lines ~52-67) to use the new return type — semantics unchanged (still gates on enablement):

```rust
    for u in users {
        let Some(rule) = first_match(&u.rules, showing) else {
            continue;
        };
        if rule.frequency == "never" {
            continue;
        }
        if u.email_enabled {
            let batch_id = db::get_or_create_open_batch(pool, u.user_id, "email", &rule.frequency).await?;
            db::append_showing_to_batch(pool, batch_id, showing_id).await?;
            affected.push((u.user_id, "email".to_string()));
        }
        if u.telegram_enabled && u.telegram_chat_id.is_some() {
            let batch_id = db::get_or_create_open_batch(pool, u.user_id, "telegram", &rule.frequency).await?;
            db::append_showing_to_batch(pool, batch_id, showing_id).await?;
            affected.push((u.user_id, "telegram".to_string()));
        }
    }
```

Update the `rule()` test helper in `batch.rs` `mod tests` to include `channels`:

```rust
    fn rule(freq: &str) -> crate::notification::rules::Rule {
        crate::notification::rules::Rule {
            cinema_id: None,
            features: vec![],
            title_substring: None,
            frequency: freq.to_string(),
            channels: vec!["email".into()],
        }
    }
```

Update the `never_match_creates_no_batch` test's inline `Rule` (currently builds a `Rule { ... frequency: "immediately".to_string() }`) to add `channels: vec!["email".into()]`.

- [ ] **Step 8: Update checker.rs test `RuleInput` usages**

In `backend/src/checker.rs`, the two tests `new_showing_routes_by_user_rules` and `new_showing_creates_batch_for_immediate_user` each build a `RuleInput`. Add `channels: vec!["email".into()]` to both:

```rust
        crate::notification::db::replace_rules(
            &pool,
            uid,
            &[crate::notification::db::RuleInput {
                cinema_id: None,
                features: vec![],
                title_substring: None,
                frequency: "immediately".into(),
                channels: vec!["email".into()],
            }],
        )
```

- [ ] **Step 9: Run all backend tests**

Run: `cd backend && cargo test`
Expected: all tests pass, including the new `replace_rules_round_trips_channels`.

- [ ] **Step 10: Commit**

```bash
git add backend/migrations/0007_channel_per_rule.sql backend/src/notification/rules.rs backend/src/notification/db.rs backend/src/notification/batch.rs backend/src/checker.rs
git commit -m "feat: add channels array to notification_rule (additive)"
```

---

### Task 2: Route by per-rule channels + API validation

Switch routing to read `rule.channels`; keep the now-unused `email_enabled`/`telegram_enabled` fields flagged `#[allow(dead_code)]` until Task 3 drops them. Add `channels` to the rules API DTOs and validate.

**Files:**
- Modify: `backend/src/notification/batch.rs`
- Modify: `backend/src/notification/api.rs`

**Interfaces:**
- Produces: `route_showing_for_users` gates on `rule.channels` (email ∈ channels → email batch; telegram ∈ channels && `telegram_chat_id.is_some()` → telegram batch). `RuleRequest { ..., channels: Vec<String> }`, `RuleResponse { ..., channels: Vec<String> }`. `validate_rules` rejects empty `channels` or any value outside `{email, telegram}`.

- [ ] **Step 1: Write failing routing tests**

Add to `backend/src/notification/batch.rs` `mod tests`:

```rust
    #[sqlx::test(migrations = "./migrations")]
    async fn telegram_only_rule_with_chat_id_routes_telegram_batch(pool: PgPool) {
        let uid = make_user(&pool, "tg1@x.com").await;
        prefs_for(&pool, uid, false, true, Some("h"), None, None).await;
        set_chat_id(&pool, uid, "12345").await;
        let sid = make_showing(&pool, "F1").await;
        let m = matchable(sid, 1, &["OV"], "F1");
        let users = vec![UserRules {
            user_id: uid,
            email_enabled: false,
            telegram_enabled: true,
            telegram_chat_id: Some("12345".into()),
            digest_anchor: at(16, 9),
            digest_hour: 9,
            rules: vec![crate::notification::rules::Rule {
                cinema_id: None,
                features: vec![],
                title_substring: None,
                frequency: "immediately".into(),
                channels: vec!["telegram".into()],
            }],
        }];
        let affected = route_showing_for_users(&pool, sid, &m, &users).await.unwrap();
        assert_eq!(affected, vec![(uid, "telegram".to_string())]);
        let n: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM notification_batch WHERE user_id=$1 AND layer='telegram' AND frequency='immediately' AND status='pending'",
        ).bind(uid).fetch_one(&pool).await.unwrap();
        assert_eq!(n.0, 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn telegram_only_rule_without_chat_id_routes_nothing(pool: PgPool) {
        let uid = make_user(&pool, "tg2@x.com").await;
        prefs_for(&pool, uid, false, true, Some("h"), None, None).await;
        // no set_chat_id — unverified
        let sid = make_showing(&pool, "F1").await;
        let m = matchable(sid, 1, &["OV"], "F1");
        let users = vec![UserRules {
            user_id: uid,
            email_enabled: false,
            telegram_enabled: true,
            telegram_chat_id: None,
            digest_anchor: at(16, 9),
            digest_hour: 9,
            rules: vec![crate::notification::rules::Rule {
                cinema_id: None,
                features: vec![],
                title_substring: None,
                frequency: "immediately".into(),
                channels: vec!["telegram".into()],
            }],
        }];
        let affected = route_showing_for_users(&pool, sid, &m, &users).await.unwrap();
        assert!(affected.is_empty());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn both_channels_routes_email_and_telegram_batches(pool: PgPool) {
        let uid = make_user(&pool, "both@x.com").await;
        prefs_for(&pool, uid, true, true, Some("h"), None, None).await;
        set_chat_id(&pool, uid, "999").await;
        let sid = make_showing(&pool, "F1").await;
        let m = matchable(sid, 1, &["OV"], "F1");
        let users = vec![UserRules {
            user_id: uid,
            email_enabled: true,
            telegram_enabled: true,
            telegram_chat_id: Some("999".into()),
            digest_anchor: at(16, 9),
            digest_hour: 9,
            rules: vec![crate::notification::rules::Rule {
                cinema_id: None,
                features: vec![],
                title_substring: None,
                frequency: "immediately".into(),
                channels: vec!["email".into(), "telegram".into()],
            }],
        }];
        let affected = route_showing_for_users(&pool, sid, &m, &users).await.unwrap();
        assert_eq!(affected, vec![(uid, "email".to_string()), (uid, "telegram".to_string())]);
    }
```

Note: the `UserRules` struct literal still uses `email_enabled`/`telegram_enabled` fields until Task 3. That's intentional.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd backend && cargo test --lib telegram_only_rule_with_chat_id_routes_telegram_batch telegram_only_rule_without_chat_id_routes_nothing both_channels_routes_email_and_telegram_batches`
Expected: the first and third FAIL (routing still gates on `u.email_enabled`/`u.telegram_enabled`, not `rule.channels`). The second passes trivially (no chat_id → already skipped).

- [ ] **Step 3: Switch routing to use `rule.channels`**

In `backend/src/notification/batch.rs`, rewrite the body of `route_showing_for_users`:

```rust
    for u in users {
        let Some(rule) = first_match(&u.rules, showing) else {
            continue;
        };
        if rule.frequency == "never" {
            continue;
        }
        if rule.channels.iter().any(|c| c == "email") {
            let batch_id = db::get_or_create_open_batch(pool, u.user_id, "email", &rule.frequency).await?;
            db::append_showing_to_batch(pool, batch_id, showing_id).await?;
            affected.push((u.user_id, "email".to_string()));
        }
        if rule.channels.iter().any(|c| c == "telegram") && u.telegram_chat_id.is_some() {
            let batch_id = db::get_or_create_open_batch(pool, u.user_id, "telegram", &rule.frequency).await?;
            db::append_showing_to_batch(pool, batch_id, showing_id).await?;
            affected.push((u.user_id, "telegram".to_string()));
        }
    }
```

Mark the now-unused `UserRules` fields `#[allow(dead_code)]` until Task 3 drops them. In `backend/src/notification/db.rs`, on `UserRules`:

```rust
#[derive(Debug, Clone)]
pub struct UserRules {
    pub user_id: i64,
    #[allow(dead_code)]
    pub email_enabled: bool,
    #[allow(dead_code)]
    pub telegram_enabled: bool,
    pub telegram_chat_id: Option<String>,
    #[allow(dead_code)]
    pub digest_anchor: DateTime<Utc>,
    #[allow(dead_code)]
    pub digest_hour: i32,
    pub rules: Vec<Rule>,
}
```

- [ ] **Step 4: Run routing tests to verify they pass**

Run: `cd backend && cargo test --lib route`
Expected: all `route_showing_for_users`-related tests pass, including the three new ones.

- [ ] **Step 5: Write failing API validation tests**

Add to `backend/src/notification/api.rs` `mod tests`:

```rust
    #[sqlx::test(migrations = "./migrations")]
    async fn put_rules_rejects_empty_channels(pool: PgPool) {
        let uid = seed_rules_user(&pool).await;
        let token = make_session(&pool, uid).await;
        let app = crate::web::router(test_state(pool));
        let resp = app.oneshot(
            Request::put("/api/preferences/rules").header("Cookie", format!("ov_session={token}"))
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(r#"{"rules":[{"cinemaId":null,"features":[],"titleSubstring":null,"frequency":"immediately","channels":[]}]}"#)).unwrap()
        ).await.unwrap();
        assert_eq!(resp.status(), 400);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn put_rules_rejects_invalid_channel(pool: PgPool) {
        let uid = seed_rules_user(&pool).await;
        let token = make_session(&pool, uid).await;
        let app = crate::web::router(test_state(pool));
        let resp = app.oneshot(
            Request::put("/api/preferences/rules").header("Cookie", format!("ov_session={token}"))
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(r#"{"rules":[{"cinemaId":null,"features":[],"titleSubstring":null,"frequency":"immediately","channels":["fax"]}]}"#)).unwrap()
        ).await.unwrap();
        assert_eq!(resp.status(), 400);
    }
```

- [ ] **Step 6: Run the validation tests to verify they fail**

Run: `cd backend && cargo test --lib put_rules_rejects_empty_channels put_rules_rejects_invalid_channel`
Expected: FAIL — `channels` not in `RuleRequest` (serde ignores unknown fields → `channels` is null/default, validation doesn't check it → 200 instead of 400).

- [ ] **Step 7: Add `channels` to API DTOs and validate**

In `backend/src/notification/api.rs`:

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleRequest {
    pub cinema_id: Option<i64>,
    pub features: Vec<String>,
    pub title_substring: Option<String>,
    pub frequency: String,
    pub channels: Vec<String>,
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
    pub channels: Vec<String>,
}
```

Update `get_rules`'s mapping (`rules.into_iter().map(|r| RuleResponse { ... })`) to pass `channels: r.channels`.

Update `put_rules`'s `RuleInput` mapping to pass `channels: r.channels`.

Add `channels` validation to `validate_rules`:

```rust
fn validate_rules(
    rules: &[RuleRequest],
    cinemas: &std::collections::HashMap<i64, String>,
) -> Result<(), StatusCode> {
    if rules.len() > MAX_RULES {
        return Err(StatusCode::BAD_REQUEST);
    }
    let is_freq = |f: &str| {
        f == "never"
            || f == "immediately"
            || matches!(f.parse::<i32>(), Ok(d) if (1..=7).contains(&d))
    };
    let valid_channels = ["email", "telegram"];
    for r in rules {
        if !is_freq(&r.frequency) {
            return Err(StatusCode::BAD_REQUEST);
        }
        if let Some(t) = &r.title_substring {
            if t.chars().count() > MAX_TITLE_LEN {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        for f in &r.features {
            if !FEATURE_VOCABULARY.contains(&f.as_str()) {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        if let Some(cid) = r.cinema_id {
            if !cinemas.contains_key(&cid) {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        if r.channels.is_empty() || !r.channels.iter().all(|c| valid_channels.contains(&c.as_str())) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    Ok(())
}
```

Update the existing `put_rules_replaces_and_rolls_over` test body to include `channels`:

```rust
        let body = r#"{"rules":[{"cinemaId":1,"features":["IMAX","Atmos"],"titleSubstring":null,"frequency":"immediately","channels":["email","telegram"]},{"cinemaId":null,"features":[],"titleSubstring":null,"frequency":"3","channels":["email"]}]}"#;
```

- [ ] **Step 8: Run all backend tests**

Run: `cd backend && cargo test`
Expected: all tests pass.

- [ ] **Step 9: Commit**

```bash
git add backend/src/notification/batch.rs backend/src/notification/api.rs backend/src/notification/db.rs
git commit -m "feat: route notifications by per-rule channels + validate"
```

---

### Task 3: Drop global enablement

Remove the now-redundant `email_enabled`/`telegram_enabled` columns and every struct field / query / test that references them. Single compile-clean commit.

**Files:**
- Create: `backend/migrations/0008_drop_global_enablement.sql`
- Modify: `backend/src/notification/db.rs`
- Modify: `backend/src/notification/api.rs`
- Modify: `backend/src/notification/verify.rs` (tests)
- Modify: `backend/src/checker.rs` (tests)
- Modify: `backend/src/notification/batch.rs` (tests)

**Interfaces:**
- Produces: `notification_preferences` no longer has `email_enabled`/`telegram_enabled`. `NotificationPreferences`, `PreferenceUpdate`, `UserRules`, `PreferenceUpdateRequest`, `PreferencesResponse` no longer have those fields. `list_active_users_with_rules` filters by `EXISTS (non-never rule)`. `put_preferences` rollover triggers on digest or telegram-handle change only.

- [ ] **Step 1: Write the failing "active = has non-never rule" test**

Add to `backend/src/notification/db.rs` `mod tests`, replacing the body of `list_active_users_with_rules_filters_inactive` to drop enablement setup (the helper signature changes in Step 4; for now this test will fail to compile against the old helper):

```rust
    #[sqlx::test(migrations = "./migrations")]
    async fn list_active_users_with_rules_filters_inactive(pool: PgPool) {
        let active = make_user(&pool, "act@x.com").await;
        let inactive = make_user(&pool, "inact@x.com").await;
        let tg_unverified = make_user(&pool, "tg@x.com").await;
        prefs_for(&pool, active, Some("h")).await;
        prefs_for(&pool, inactive, None).await;
        prefs_for(&pool, tg_unverified, Some("h2")).await;
        replace_rules(
            &pool,
            active,
            &[RuleInput {
                cinema_id: None,
                features: vec![],
                title_substring: None,
                frequency: "3".into(),
                channels: vec!["email".into()],
            }],
        )
        .await
        .unwrap();
        replace_rules(
            &pool,
            inactive,
            &[RuleInput {
                cinema_id: None,
                features: vec![],
                title_substring: None,
                frequency: "never".into(),
                channels: vec!["email".into()],
            }],
        )
        .await
        .unwrap();

        let users = list_active_users_with_rules(&pool).await.unwrap();
        let ids: Vec<i64> = users.iter().map(|u| u.user_id).collect();
        assert!(ids.contains(&active));
        assert!(!ids.contains(&inactive), "only-never-rule user is inactive");
        assert!(
            !ids.contains(&tg_unverified),
            "user with no non-never rule must not be active"
        );
        let a = users.iter().find(|u| u.user_id == active).unwrap();
        assert_eq!(a.rules.len(), 1);
        assert_eq!(a.rules[0].frequency, "3");
    }
```

- [ ] **Step 2: Run to confirm it fails**

Run: `cd backend && cargo test --lib list_active_users_with_rules_filters_inactive`
Expected: compile error (old `prefs_for` signature) — this drives the refactor.

- [ ] **Step 3: Create the drop migration**

Create `backend/migrations/0008_drop_global_enablement.sql`:

```sql
ALTER TABLE notification_preferences
  DROP COLUMN email_enabled,
  DROP COLUMN telegram_enabled;
```

- [ ] **Step 4: Drop enablement from db.rs structs + queries**

In `backend/src/notification/db.rs`:

`NotificationPreferences`:
```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct NotificationPreferences {
    pub user_id: i64,
    pub telegram_handle: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub digest_anchor: DateTime<Utc>,
    pub digest_hour: i32,
    #[allow(dead_code)]
    pub updated_at: DateTime<Utc>,
}
```

`PreferenceUpdate`:
```rust
#[derive(Debug, Default)]
pub struct PreferenceUpdate {
    pub telegram_handle: Option<String>,
    pub digest_anchor: Option<DateTime<Utc>>,
    pub digest_hour: Option<i32>,
}
```

`UserRules` — remove the `#[allow(dead_code)]` flags added in Task 2 and drop the fields:
```rust
#[derive(Debug, Clone)]
pub struct UserRules {
    pub user_id: i64,
    pub telegram_chat_id: Option<String>,
    #[allow(dead_code)]
    pub digest_anchor: DateTime<Utc>,
    #[allow(dead_code)]
    pub digest_hour: i32,
    pub rules: Vec<Rule>,
}
```

`From<NotificationPreferences> for UserRules`:
```rust
impl From<NotificationPreferences> for UserRules {
    fn from(p: NotificationPreferences) -> Self {
        UserRules {
            user_id: p.user_id,
            telegram_chat_id: p.telegram_chat_id,
            digest_anchor: p.digest_anchor,
            digest_hour: p.digest_hour,
            rules: Vec::new(),
        }
    }
}
```

`list_active_users_with_rules`:
```rust
pub async fn list_active_users_with_rules(pool: &PgPool) -> sqlx::Result<Vec<UserRules>> {
    let prefs: Vec<NotificationPreferences> = sqlx::query_as(
        "SELECT user_id, telegram_handle, telegram_chat_id, digest_anchor, digest_hour, updated_at
         FROM notification_preferences p
         WHERE EXISTS (
           SELECT 1 FROM notification_rule r
           WHERE r.user_id = p.user_id AND r.frequency <> 'never'
         )
         ORDER BY user_id",
    )
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(prefs.len());
    for p in prefs {
        let rules: Vec<NotificationRule> = sqlx::query_as(
            "SELECT id, user_id, position, cinema_id, features, title_substring, frequency, channels
             FROM notification_rule WHERE user_id = $1 ORDER BY position",
        )
        .bind(p.user_id)
        .fetch_all(pool)
        .await?;
        out.push(UserRules {
            user_id: p.user_id,
            telegram_chat_id: p.telegram_chat_id,
            digest_anchor: p.digest_anchor,
            digest_hour: p.digest_hour,
            rules: rules
                .into_iter()
                .map(|r| Rule {
                    cinema_id: r.cinema_id,
                    features: r.features,
                    title_substring: r.title_substring,
                    frequency: r.frequency,
                    channels: r.channels,
                })
                .collect(),
        });
    }
    Ok(out)
}
```

`get_preferences`:
```rust
pub async fn get_preferences(pool: &PgPool, user_id: i64) -> sqlx::Result<NotificationPreferences> {
    let created_at: Option<(DateTime<Utc>,)> =
        sqlx::query_as("SELECT created_at FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    let Some((created_at,)) = created_at else {
        return Err(sqlx::Error::RowNotFound);
    };
    let row = sqlx::query_as::<_, NotificationPreferences>(
        "SELECT user_id, telegram_handle, telegram_chat_id, digest_anchor, digest_hour, updated_at
         FROM notification_preferences WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(match row {
        Some(prefs) => prefs,
        None => NotificationPreferences {
            user_id,
            telegram_handle: None,
            telegram_chat_id: None,
            digest_anchor: created_at,
            digest_hour: 9,
            updated_at: created_at,
        },
    })
}
```

`upsert_preferences`:
```rust
pub async fn upsert_preferences(
    pool: &PgPool,
    user_id: i64,
    dto: PreferenceUpdate,
) -> sqlx::Result<NotificationPreferences> {
    let existing = get_preferences(pool, user_id).await?;
    let new_handle = dto
        .telegram_handle
        .map(|raw| raw.trim().trim_start_matches('@').to_lowercase())
        .filter(|h| !h.is_empty());
    let clear_chat =
        new_handle.is_none() || new_handle.as_deref() != existing.telegram_handle.as_deref();
    let chat_id = if clear_chat {
        None
    } else {
        existing.telegram_chat_id.clone()
    };
    let digest_anchor = dto.digest_anchor.unwrap_or(existing.digest_anchor);
    let digest_hour = dto.digest_hour.unwrap_or(existing.digest_hour);
    sqlx::query_as::<_, NotificationPreferences>(
        "INSERT INTO notification_preferences
           (user_id, telegram_handle, telegram_chat_id, digest_anchor, digest_hour, updated_at)
         VALUES ($1, $2, $3, $4, $5, now())
         ON CONFLICT (user_id) DO UPDATE SET
           telegram_handle = EXCLUDED.telegram_handle,
           telegram_chat_id = EXCLUDED.telegram_chat_id,
           digest_anchor   = EXCLUDED.digest_anchor,
           digest_hour     = EXCLUDED.digest_hour,
           updated_at      = now()
         RETURNING user_id, telegram_handle, telegram_chat_id, digest_anchor, digest_hour, updated_at",
    )
    .bind(user_id)
    .bind(new_handle)
    .bind(chat_id)
    .bind(digest_anchor)
    .bind(digest_hour)
    .fetch_one(pool)
    .await
}
```

Update the db.rs test `prefs_for` helper (drop enablement args):
```rust
    async fn prefs_for(pool: &PgPool, uid: i64, handle: Option<&str>) {
        upsert_preferences(
            pool,
            uid,
            PreferenceUpdate {
                telegram_handle: handle.map(|s| s.to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }
```

Update `preferences_defaults_and_upsert`, `get_preferences_defaults_derive_anchor_from_user`, `upsert_clears_chat_id_on_handle_change_or_clear` to drop `email_enabled`/`telegram_enabled` from `PreferenceUpdate` literals and from assertions (e.g. `assert!(!prefs.email_enabled);` lines are deleted; `assert!(updated.email_enabled);` → deleted).

For `upsert_clears_chat_id_on_handle_change_or_clear`, the first `upsert_preferences` block becomes:
```rust
        upsert_preferences(
            &pool,
            uid,
            PreferenceUpdate {
                telegram_handle: Some("myhandle".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
```

For `get_due_batches_reads_batch_frequency` and `get_due_batches_returns_pending_and_retryable_failed`, replace `prefs_for(&pool, uid, true, false, None, Some(anchor), Some(9))` with `prefs_for(&pool, uid, None).await` and (if the test sets a digest anchor) add a separate `upsert_preferences` with `digest_anchor`/`digest_hour`. For these two batch-due tests, the digest anchor comes from the user's `created_at` fallback, which is fine — replace the `prefs_for` call with `prefs_for(&pool, uid, None).await`.

- [ ] **Step 5: Drop enablement from api.rs**

In `backend/src/notification/api.rs`:

`PreferenceUpdateRequest`:
```rust
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceUpdateRequest {
    pub telegram_handle: Option<String>,
    pub digest_anchor: Option<DateTime<Utc>>,
    pub digest_hour: Option<i32>,
}

impl From<PreferenceUpdateRequest> for crate::notification::db::PreferenceUpdate {
    fn from(req: PreferenceUpdateRequest) -> Self {
        Self {
            telegram_handle: req.telegram_handle,
            digest_anchor: req.digest_anchor,
            digest_hour: req.digest_hour,
        }
    }
}
```

`PreferencesResponse`:
```rust
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telegram_handle: Option<String>,
    pub telegram_verified: bool,
    pub digest_anchor: DateTime<Utc>,
    pub digest_hour: i32,
}

impl From<crate::notification::db::NotificationPreferences> for PreferencesResponse {
    fn from(p: crate::notification::db::NotificationPreferences) -> Self {
        PreferencesResponse {
            telegram_handle: p.telegram_handle,
            telegram_verified: p.telegram_chat_id.is_some(),
            digest_anchor: p.digest_anchor,
            digest_hour: p.digest_hour,
        }
    }
}
```

`put_preferences` rollover block — replace lines ~88-102:
```rust
    let changed_digest = dto.digest_anchor.is_some() || dto.digest_hour.is_some();
    let changed_handle = dto.telegram_handle.is_some();
    let updated = crate::notification::db::upsert_preferences(&state.pool, auth.user_id, dto)
        .await
        .map_err(|e| {
            tracing::error!("upsert_preferences failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if changed_digest {
        rollover_batch(&state, auth.user_id, "email").await;
    }
    if changed_digest || changed_handle {
        rollover_batch(&state, auth.user_id, "telegram").await;
    }
    Ok(Json(updated.into()))
```

`delete_telegram` — replace the `PreferenceUpdate` literal:
```rust
    crate::notification::db::upsert_preferences(
        &state.pool,
        auth.user_id,
        crate::notification::db::PreferenceUpdate {
            telegram_handle: Some(String::new()),
            ..Default::default()
        },
    )
```

Update api.rs tests:
- `get_preferences_defaults`: delete the `json["emailEnabled"]`/`json["telegramEnabled"]` assertions. Keep the rest.
- `put_preferences_updates_and_returns_values`: change the request body to `r#"{"telegramHandle":"@MyHandle","digestHour":10}"#` and delete the `json["emailEnabled"]`/`json["telegramEnabled"]` assertions (keep `telegramHandle`/`telegramVerified`/`digestHour`).
- `put_preferences_accepts_valid_enablement`: rename to `put_preferences_accepts_handle_update` and change the body to `r#"{"telegramHandle":"@newhandle"}"#`.
- `put_preferences_rolls_over_open_batches`: the request body becomes `r#"{"telegramHandle":"@h"}"#`; assert only the telegram batch is rolled over (email batch is no longer affected by a handle-only change). Keep the email-batch-count assertion at 1 (created earlier, not rolled over) and telegram at 0.
- `delete_telegram_clears_handle_and_sets_never`: rename to `delete_telegram_clears_handle`; the existing assertions on `telegramEnabled`/`telegramHandle`/`telegramVerified` become `telegramHandle == null` and `telegramVerified == false`.

- [ ] **Step 6: Update verify.rs and checker.rs tests**

In `backend/src/notification/verify.rs`, the `webhook_verifies_handle_and_stores_chat_id` test's `PreferenceUpdate` becomes:
```rust
        crate::notification::db::upsert_preferences(
            &pool,
            uid,
            crate::notification::db::PreferenceUpdate {
                telegram_handle: Some("myhandle".into()),
                ..Default::default()
            },
        )
```

In `backend/src/checker.rs`, both `new_showing_routes_by_user_rules` and `new_showing_creates_batch_for_immediate_user` drop the `upsert_preferences` block entirely (the user is active via the rule; no enablement to set). Delete these blocks:
```rust
        crate::notification::db::upsert_preferences(
            &pool,
            uid,
            crate::notification::db::PreferenceUpdate {
                email_enabled: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
```

- [ ] **Step 7: Update batch.rs test helpers**

In `backend/src/notification/batch.rs` `mod tests`, the `prefs_for` helper:
```rust
    async fn prefs_for(pool: &PgPool, uid: i64, _email: bool, _tg: bool, handle: Option<&str>, _anchor: Option<()>, _hour: Option<()>) {
        // email/tg enablement dropped; only the handle matters now.
        upsert_preferences(
            pool,
            uid,
            PreferenceUpdate {
                telegram_handle: handle.map(|s| s.to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }
```

Hmm — but every existing `prefs_for` call site passes the old 7 args. Changing the arity touches every call. Simpler: keep the same arity but make enablement params no-ops (`_email: bool, _tg: bool`) and anchor/hour params ignored (`_anchor: Option<DateTime<Utc>>, _hour: Option<i32>`). The call sites stay valid as-is.

Actually, the existing `prefs_for` signature is `(pool, uid, email_enabled, telegram_enabled, handle, digest_anchor, digest_hour)`. Keep those params but ignore the enablement/anchor/hour ones:

```rust
    async fn prefs_for(
        pool: &PgPool,
        uid: i64,
        _email_enabled: bool,
        _telegram_enabled: bool,
        handle: Option<&str>,
        _digest_anchor: Option<DateTime<Utc>>,
        _digest_hour: Option<i32>,
    ) {
        upsert_preferences(
            pool,
            uid,
            PreferenceUpdate {
                telegram_handle: handle.map(|s| s.to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }
```

The `user_rules()` helper in batch.rs builds a `UserRules` literal with `email_enabled`/`telegram_enabled` fields — those fields no longer exist. Update it:
```rust
    fn user_rules(
        uid: i64,
        _email: bool,
        _tg: bool,
        chat: Option<&str>,
        rules: Vec<crate::notification::rules::Rule>,
    ) -> crate::notification::db::UserRules {
        crate::notification::db::UserRules {
            user_id: uid,
            telegram_chat_id: chat.map(|s| s.to_string()),
            digest_anchor: at(16, 9),
            digest_hour: 9,
            rules,
        }
    }
```

The three new tests added in Task 2 also build `UserRules { email_enabled, telegram_enabled, ... }` literals — update them to drop those two fields (keep `telegram_chat_id`, `digest_anchor`, `digest_hour`, `rules`).

The Task 2 tests `telegram_only_rule_with_chat_id_routes_telegram_batch`, `telegram_only_rule_without_chat_id_routes_nothing`, `both_channels_routes_email_and_telegram_batches` each had:
```rust
        let users = vec![UserRules {
            user_id: uid,
            email_enabled: ...,
            telegram_enabled: ...,
            telegram_chat_id: ...,
            digest_anchor: at(16, 9),
            digest_hour: 9,
            rules: vec![...],
        }];
```
Drop the `email_enabled`/`telegram_enabled` lines.

- [ ] **Step 8: Run all backend tests**

Run: `cd backend && cargo test`
Expected: all tests pass.

- [ ] **Step 9: Commit**

```bash
git add backend/migrations/0008_drop_global_enablement.sql backend/src/notification/db.rs backend/src/notification/api.rs backend/src/notification/verify.rs backend/src/checker.rs backend/src/notification/batch.rs
git commit -m "feat: drop global email/telegram enablement (channel is per-rule)"
```

---

### Task 4: Frontend functional rework (types, API, page logic, i18n, tests)

Wire the frontend to the new channel-per-rule API: drop enablement from the prefs type, add `channel` to the rule type, convert `channel ↔ channels[]` at the API boundary, rework the Preferences page (remove toggle cards, add compact Telegram-Konto card, add channel `<select>`, default `channel: "both"`, warning badge), add i18n keys, update tests. CSS is intentionally untouched in this task (Task 5 fixes styling) — the rule UI stays visually broken until then, same as today.

**Files:**
- Modify: `frontend/src/types.ts`
- Modify: `frontend/src/api/preferences.ts`
- Modify: `frontend/src/pages/PreferencesPage.tsx`
- Modify: `frontend/src/locales/en.json`
- Modify: `frontend/src/locales/de.json`
- Modify: `frontend/src/pages/PreferencesPage.test.tsx`

**Interfaces:**
- Produces: `NotificationPreferences` (no enablement fields); `NotificationRule.channel: "email" | "telegram" | "both"`; `savePreferences` sends `{telegramHandle}`; `saveRules` sends `channels` array; `fetchRules` returns rules with `channel` populated.

- [ ] **Step 1: Write the failing channel-select test**

Replace `frontend/src/pages/PreferencesPage.test.tsx` entirely:

```tsx
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import i18n from "../i18n";
import { AuthProvider } from "../hooks/useAuth";
import { PreferencesPage } from "./PreferencesPage";
import type { NotificationPreferences } from "../types";

const mockPrefs: NotificationPreferences = {
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
      if (init && init.method === "PUT") return { ok: true, json: async () => ({ rules: JSON.parse(String(init.body)).rules, cinemas: [{ id: 1, name: "Cineplexx Linz" }, { id: 2, name: "Megaplex PlusCity" }] }) };
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

function renderPage() {
  return render(
    <MemoryRouter>
      <AuthProvider>
        <PreferencesPage />
      </AuthProvider>
    </MemoryRouter>
  );
}

afterEach(() => vi.unstubAllGlobals());
beforeEach(() => i18n.changeLanguage("en"));

describe("PreferencesPage", () => {
  it("shows a loading state while preferences are being fetched", async () => {
    mockFetch(mockPrefs);
    renderPage();
    expect(screen.getByText("Loading…")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Notification preferences" })).toBeInTheDocument();
  });

  it("renders the Telegram handle input bound to the fetched handle", async () => {
    mockFetch({ ...mockPrefs, telegramHandle: "myhandle" });
    renderPage();
    await screen.findByRole("heading", { name: "Notification preferences" });
    expect(screen.getByLabelText("Telegram handle")).toHaveValue("myhandle");
  });

  it("adds a rule, picks a channel, and saves the mapped channels array", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.startsWith("/api/auth/me")) return { ok: true, json: async () => ({ id: 1, email: "a@b.c" }) };
      if (url.startsWith("/api/auth/providers")) return { ok: true, json: async () => ({ email: true, google: true, github: true, dev: false }) };
      if (url.startsWith("/api/preferences/rules")) {
        if (init && init.method === "PUT") return { ok: true, json: async () => ({ rules: JSON.parse(String(init.body)).rules, cinemas: [{ id: 1, name: "Cineplexx Linz" }, { id: 2, name: "Megaplex PlusCity" }] }) };
        return { ok: true, json: async () => ({ rules: [], cinemas: [{ id: 1, name: "Cineplexx Linz" }, { id: 2, name: "Megaplex PlusCity" }] }) };
      }
      if (url.startsWith("/api/preferences")) return { ok: true, json: async () => mockPrefs };
      return { ok: false, status: 404 };
    });
    vi.stubGlobal("fetch", fetchMock);
    renderPage();
    await screen.findByRole("heading", { name: "Notification preferences" });
    fireEvent.click(screen.getByRole("button", { name: "Add rule" }));
    const channel = await screen.findByLabelText("Rule 1 channel");
    fireEvent.change(channel, { target: { value: "telegram" } });
    fireEvent.click(screen.getByRole("button", { name: "Save rules" }));
    await waitFor(() => {
      const put = fetchMock.mock.calls.find(([u, i]) => String(u).startsWith("/api/preferences/rules") && i && i.method === "PUT");
      expect(put).toBeDefined();
      const body = JSON.parse(String(put![1]!.body));
      expect(body.rules[0].channels).toEqual(["telegram"]);
    });
  });

  it("shows the loadError text when fetching preferences fails", async () => {
    mockFetch(new Error("boom"));
    renderPage();
    expect(await screen.findByText("Could not load preferences.")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npm test -- --run`
Expected: FAIL — `NotificationPreferences` still has `emailEnabled`/`telegramEnabled`; `NotificationRule` has no `channel`; `channel` select doesn't exist.

- [ ] **Step 3: Update `types.ts`**

```ts
export interface NotificationPreferences {
  telegramHandle: string;
  telegramVerified: boolean;
  digestAnchor: string;
  digestHour: number;
}

export type NotificationChannel = "email" | "telegram" | "both";

export interface NotificationRule {
  id?: number;
  position: number;
  cinemaId: number | null;
  cinemaName?: string | null;
  features: string[];
  titleSubstring: string | null;
  frequency: NotificationFrequency;
  channel: NotificationChannel;
}
```

- [ ] **Step 4: Update `api/preferences.ts` with channel conversion**

```ts
import type { NotificationPreferences, RulesResponse, NotificationRule, NotificationChannel, Cinema } from "../types";

type WireRule = Omit<NotificationRule, "channel"> & { channels: string[] };

function channelsToChannel(channels: string[]): NotificationChannel {
  const hasEmail = channels.includes("email");
  const hasTelegram = channels.includes("telegram");
  if (hasEmail && hasTelegram) return "both";
  if (hasTelegram) return "telegram";
  return "email";
}

function channelToChannels(channel: NotificationChannel): string[] {
  if (channel === "both") return ["email", "telegram"];
  return [channel];
}

export async function fetchPreferences(): Promise<NotificationPreferences> {
  const res = await fetch("/api/preferences", { credentials: "include" });
  if (!res.ok) throw new Error("failed to load preferences");
  return res.json();
}

export async function savePreferences(prefs: Pick<NotificationPreferences, "telegramHandle">): Promise<NotificationPreferences> {
  const res = await fetch("/api/preferences", {
    method: "PUT", headers: { "Content-Type": "application/json" },
    credentials: "include", body: JSON.stringify({ telegramHandle: prefs.telegramHandle }),
  });
  if (!res.ok) throw new Error("failed to save preferences");
  return res.json();
}

export async function fetchRules(): Promise<RulesResponse> {
  const res = await fetch("/api/preferences/rules", { credentials: "include" });
  if (!res.ok) throw new Error("failed to load rules");
  const data: { rules: WireRule[]; cinemas: Cinema[] } = await res.json();
  return {
    rules: data.rules.map((r) => ({ ...r, channel: channelsToChannel(r.channels) })),
    cinemas: data.cinemas,
  };
}

export async function saveRules(rules: NotificationRule[]): Promise<RulesResponse> {
  const res = await fetch("/api/preferences/rules", {
    method: "PUT", headers: { "Content-Type": "application/json" },
    credentials: "include",
    body: JSON.stringify({ rules: rules.map((r) => ({ cinemaId: r.cinemaId, features: r.features, titleSubstring: r.titleSubstring, frequency: r.frequency, channels: channelToChannels(r.channel) })) }),
  });
  if (!res.ok) throw new Error("failed to save rules");
  const data: { rules: WireRule[]; cinemas: Cinema[] } = await res.json();
  return {
    rules: data.rules.map((r) => ({ ...r, channel: channelsToChannel(r.channels) })),
    cinemas: data.cinemas,
  };
}
```

- [ ] **Step 5: Add i18n keys**

In `frontend/src/locales/en.json` under `preferences`, add (and soften `telegramDesc`):

```json
    "telegramDesc": "Link your Telegram account so rules that pick Telegram can fire.",
    "channel": "Channel",
    "channelEmail": "Email",
    "channelTelegram": "Telegram",
    "channelBoth": "Both",
    "telegramUnverified": "Telegram not linked — this rule will only email until you verify.",
```

In `frontend/src/locales/de.json` under `preferences`:

```json
    "telegramDesc": "Verknüpfe dein Telegram-Konto, damit Regeln mit Telegram benachrichtigen können.",
    "channel": "Kanal",
    "channelEmail": "E-Mail",
    "channelTelegram": "Telegram",
    "channelBoth": "Beide",
    "telegramUnverified": "Telegram nicht verknüpft — diese Regel benachrichtigt nur per E-Mail, bis du verknüpfst.",
```

- [ ] **Step 6: Rework `PreferencesPage.tsx`**

Replace the file body. Remove the email/telegram toggle cards (old lines 67-112); add a compact Telegram-Konto card; add the channel `<select>` to each rule row; default new rules to `channel: "both"`; show a `.rule-warn` badge when a rule selects Telegram/Both and the account is unverified; wrap Add/Save buttons in `.pref-actions`.

```tsx
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { Marquee } from "../components/Marquee";
import { FEATURES, FREQUENCY_OPTIONS, type NotificationChannel, type NotificationFrequency, type NotificationPreferences, type NotificationRule, type Cinema } from "../types";
import { fetchPreferences, savePreferences, fetchRules, saveRules } from "../api/preferences";

function frequencyLabel(t: TFunction, value: NotificationFrequency): string {
  if (value === "never") return t("preferences.frequencies.never");
  if (value === "immediately") return t("preferences.frequencies.immediately");
  return t("preferences.frequencies.days", { count: Number(value) });
}

export function PreferencesPage() {
  const { t } = useTranslation();
  const [prefs, setPrefs] = useState<NotificationPreferences | null>(null);
  const [rules, setRules] = useState<NotificationRule[]>([]);
  const [cinemas, setCinemas] = useState<Cinema[]>([]);
  const [loading, setLoading] = useState(true);
  const [saved, setSaved] = useState(false);
  const [rulesSaved, setRulesSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    Promise.all([fetchPreferences(), fetchRules()])
      .then(([p, r]) => {
        if (!cancelled) {
          setPrefs(p);
          setRules(r.rules);
          setCinemas(r.cinemas);
        }
      })
      .catch(() => { if (!cancelled) setError(t("preferences.loadError")); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [t]);

  useEffect(() => {
    if (!saved) return;
    const id = setTimeout(() => setSaved(false), 2000);
    return () => clearTimeout(id);
  }, [saved]);

  const handleSave = async () => {
    if (!prefs) return;
    try {
      const updated = await savePreferences({ telegramHandle: prefs.telegramHandle });
      setPrefs(updated);
      setSaved(true);
      setError(null);
    } catch {
      setError(t("preferences.saveError"));
    }
  };

  const addRule = () => setRules([...rules, { position: rules.length, cinemaId: null, features: [], titleSubstring: null, frequency: "3", channel: "both" }]);
  const removeRule = (i: number) => setRules(rules.filter((_, idx) => idx !== i).map((r, idx) => ({ ...r, position: idx })));
  const updateRule = (i: number, patch: Partial<NotificationRule>) => setRules(rules.map((r, idx) => idx === i ? { ...r, ...patch } : r));
  const toggleFeature = (i: number, f: string) => setRules(rules.map((r, idx) => idx === i ? { ...r, features: r.features.includes(f) ? r.features.filter((x) => x !== f) : [...r.features, f] } : r));
  const handleSaveRules = async () => { const res = await saveRules(rules); setRules(res.rules); setRulesSaved(true); };

  if (loading) return <div className="preferences"><Marquee /><p>{t("preferences.loading")}</p></div>;
  if (error) return <div className="preferences"><Marquee /><p className="pref-error">{error}</p></div>;
  if (!prefs) return null;

  const telegramUnverified = !prefs.telegramVerified;
  const channels: NotificationChannel[] = ["email", "telegram", "both"];

  return (
    <div className="preferences">
      <Marquee />
      <h2>{t("preferences.title")}</h2>
      <div className="card pref-card">
        <h3>{t("preferences.telegram")}</h3>
        <p className="pref-desc">{t("preferences.telegramDesc")}</p>
        <label className="pref-field">
          <span>{t("preferences.telegramHandle")}</span>
          <input
            className="pref-input"
            type="text"
            placeholder={t("preferences.telegramHandlePlaceholder")}
            value={prefs.telegramHandle ?? ""}
            onChange={(e) => setPrefs({ ...prefs, telegramHandle: e.target.value })}
            aria-label={t("preferences.telegramHandle")}
          />
        </label>
        <div className="pref-telegram-status">
          {prefs.telegramVerified ? (
            <span className="pref-verified">{t("preferences.telegramVerified")}</span>
          ) : prefs.telegramHandle ? (
            <span className="pref-verify-prompt">{t("preferences.telegramVerifyPrompt")}</span>
          ) : null}
        </div>
      </div>
      <div className="pref-actions">
        <button className="auth-submit" onClick={handleSave}>{t("preferences.save")}</button>
        {saved && <span className="pref-saved">{t("preferences.saved")}</span>}
      </div>
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
            <select aria-label={"Rule " + (i + 1) + " channel"} value={r.channel} onChange={(e) => updateRule(i, { channel: e.target.value as NotificationChannel })}>
              {channels.map((c) => <option key={c} value={c}>{t("preferences.channel" + (c.charAt(0).toUpperCase() + c.slice(1)))}</option>)}
            </select>
            <select aria-label={"Rule " + (i + 1) + " frequency"} value={r.frequency} onChange={(e) => updateRule(i, { frequency: e.target.value as NotificationFrequency })}>
              {FREQUENCY_OPTIONS.map((v) => <option key={v} value={v}>{frequencyLabel(t, v)}</option>)}
            </select>
            <button className="rule-remove" onClick={() => removeRule(i)}>x</button>
          </div>
          <div className="rule-features">
            {FEATURES.map((f) => (
              <button key={f} className={"chip " + (r.features.includes(f) ? "chip-on" : "")} onClick={() => toggleFeature(i, f)}>{f}</button>
            ))}
            {(r.channel === "telegram" || r.channel === "both") && telegramUnverified && (
              <span className="rule-warn">{t("preferences.telegramUnverified")}</span>
            )}
          </div>
        </div>
      ))}
      <div className="pref-actions">
        <button className="auth-submit" onClick={addRule}>{t("preferences.addRule")}</button>
        <button className="auth-submit" onClick={handleSaveRules}>{t("preferences.saveRules")}</button>
        {rulesSaved && <span className="pref-saved">{t("preferences.saved")}</span>}
      </div>
    </div>
  );
}
```

- [ ] **Step 7: Run frontend tests + build**

Run: `cd frontend && npm test -- --run && npm run build`
Expected: tests pass; build succeeds. (The rule UI is still visually unstyled — Task 5 fixes CSS.)

- [ ] **Step 8: Commit**

```bash
git add frontend/src/types.ts frontend/src/api/preferences.ts frontend/src/pages/PreferencesPage.tsx frontend/src/locales/en.json frontend/src/locales/de.json frontend/src/pages/PreferencesPage.test.tsx
git commit -m "feat: per-rule channel selector + drop global enablement toggles in UI"
```

---

### Task 5: Fix rule-card CSS

Add the missing styles for the rule row, chips, remove button, channel warning, and responsive layout. Pure CSS — no logic change.

**Files:**
- Modify: `frontend/src/index.css`

**Interfaces:**
- Consumes: class names emitted by `PreferencesPage.tsx` (Task 4): `.rule-row`, `.rule-features`, `.chip`, `.chip-on`, `.rule-remove`, `.rule-warn`, `.pref-actions` (already exists), `.pref-input`/`.pref-select` (already exist).

- [ ] **Step 1: Add the rule-card styles**

Append to `frontend/src/index.css` (inside the existing leading-space indented block, after the `.ignore-error` rule):

```css
   .rule-row{display:flex;flex-wrap:wrap;gap:.5rem;align-items:center}
   .rule-row select,.rule-row input{
    background:var(--bg);border:1px solid var(--edge);color:var(--text);
    border-radius:4px;padding:.3rem .5rem;font-size:.8rem;
   }
   .rule-row select:focus,.rule-row input:focus{outline:none;border-color:var(--gold)}
   .rule-remove{
    background:transparent;border:1px solid var(--edge);color:var(--err);
    border-radius:4px;padding:.15rem .45rem;font-size:.75rem;cursor:pointer;
    margin-left:auto;
   }
   .rule-remove:hover{border-color:var(--err);background:rgba(224,122,106,.12)}
   .rule-features{display:flex;flex-wrap:wrap;gap:.35rem;align-items:center;margin-top:.5rem}
   .chip{
    background:var(--panel);border:1px solid var(--edge);color:var(--dim);
    border-radius:999px;padding:.2rem .6rem;font-size:.72rem;cursor:pointer;
    transition:background .12s ease,border-color .12s ease,color .12s ease;
   }
   .chip:hover{color:var(--text);border-color:var(--gold)}
   .chip-on{background:rgba(232,179,77,.18);border-color:var(--gold);color:var(--gold-bright)}
   .rule-warn{color:var(--err);font-size:.7rem;margin-left:.4rem}
   @media (max-width:560px){
    .rule-row select,.rule-row input{flex:1 1 100%}
    .rule-remove{margin-left:0;flex:0 0 auto}
   }
```

- [ ] **Step 2: Run build to verify CSS compiles**

Run: `cd frontend && npm run build`
Expected: build succeeds.

- [ ] **Step 3: Run tests to verify no regressions**

Run: `cd frontend && npm test -- --run`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/index.css
git commit -m "style: rule-card chips, channel row, responsive layout"
```

---

## Verification

After all tasks:

- `cd backend && cargo fmt --check && cargo clippy -- -D warnings && cargo test` (needs `docker compose up -d db` + `DATABASE_URL`).
- `cd frontend && npm test && npm run build`.
- Manual: start the app, open Preferences, add a rule, pick "Both", confirm the chips toggle, the warning badge appears when Telegram isn't linked, and saving persists the rule.
