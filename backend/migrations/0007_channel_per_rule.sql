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
