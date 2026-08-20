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
