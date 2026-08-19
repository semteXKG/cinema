CREATE TABLE movie_ignore (
    user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    cinema     TEXT NOT NULL,
    title      TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, cinema, title)
);

CREATE INDEX idx_movie_ignore_user ON movie_ignore(user_id);
