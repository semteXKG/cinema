CREATE TABLE notification_preferences (
  user_id              BIGINT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
  email_frequency      TEXT NOT NULL DEFAULT 'never',
  telegram_frequency   TEXT NOT NULL DEFAULT 'never',
  telegram_handle      TEXT,
  telegram_chat_id     TEXT,
  digest_anchor        TIMESTAMPTZ NOT NULL DEFAULT now(),
  digest_hour          INT NOT NULL DEFAULT 9 CHECK (digest_hour BETWEEN 0 AND 23),
  updated_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE notification_batch (
  id           BIGSERIAL PRIMARY KEY,
  user_id      BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  layer        TEXT NOT NULL CHECK (layer IN ('email', 'telegram')),
  status       TEXT NOT NULL DEFAULT 'pending'
               CHECK (status IN ('pending', 'sending', 'sent', 'failed')),
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  sent_at      TIMESTAMPTZ,
  error_count  INT NOT NULL DEFAULT 0,
  last_error   TEXT
);

CREATE UNIQUE INDEX idx_batch_open_unique
  ON notification_batch(user_id, layer)
  WHERE status = 'pending';

CREATE INDEX idx_batch_status ON notification_batch(user_id, layer, status)
  WHERE status IN ('pending', 'sending', 'failed');

CREATE TABLE notification_batch_showing (
  batch_id     BIGINT NOT NULL REFERENCES notification_batch(id) ON DELETE CASCADE,
  showing_id   BIGINT NOT NULL REFERENCES showing(id) ON DELETE CASCADE,
  added_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (batch_id, showing_id)
);
