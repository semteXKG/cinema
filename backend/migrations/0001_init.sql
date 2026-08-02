CREATE TABLE movie (
  id          BIGSERIAL PRIMARY KEY,
  cinema      TEXT NOT NULL,
  title       TEXT NOT NULL,
  runtime_min INT,
  genres      TEXT[] NOT NULL DEFAULT '{}',
  poster_url  TEXT,
  poster_file TEXT,
  UNIQUE (cinema, title)
);

CREATE TABLE showing (
  id            BIGSERIAL PRIMARY KEY,
  movie_id      BIGINT NOT NULL REFERENCES movie(id) ON DELETE CASCADE,
  start         TIMESTAMPTZ NOT NULL,
  version       TEXT NOT NULL,
  hall          TEXT NOT NULL DEFAULT '',
  url           TEXT NOT NULL DEFAULT '',
  first_seen_at TIMESTAMPTZ NOT NULL,
  UNIQUE (movie_id, start)
);

CREATE TABLE source_status (
  source               TEXT PRIMARY KEY,
  status               TEXT NOT NULL,
  last_error_ping_date DATE
);

CREATE TABLE check_run (
  id          BIGSERIAL PRIMARY KEY,
  run_at      TIMESTAMPTZ NOT NULL,
  new_count   INT NOT NULL,
  total_count INT NOT NULL
);
