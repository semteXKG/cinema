-- Cinema lookup table (replaces duplicated cinema text columns)
CREATE TABLE cinema (
    id   BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

-- Seed known cinemas (matches fetcher constants CINEPLEXX_CINEMA_NAME / MEGAPLEX_CINEMA_NAME)
INSERT INTO cinema (name) VALUES
    ('Cineplexx Linz'),
    ('Megaplex PlusCity')
ON CONFLICT DO NOTHING;

-- movie: add cinema_id, backfill, drop cinema text column
ALTER TABLE movie ADD COLUMN cinema_id BIGINT REFERENCES cinema(id);
UPDATE movie m SET cinema_id = c.id FROM cinema c WHERE m.cinema = c.name;
ALTER TABLE movie ALTER COLUMN cinema_id SET NOT NULL;
ALTER TABLE movie DROP COLUMN cinema;
ALTER TABLE movie ADD UNIQUE (cinema_id, title);

-- movie_ignore: add cinema_id, backfill, drop cinema text column
-- Must drop PK first (it includes the cinema column)
ALTER TABLE movie_ignore DROP CONSTRAINT movie_ignore_pkey;
ALTER TABLE movie_ignore ADD COLUMN cinema_id BIGINT REFERENCES cinema(id);
UPDATE movie_ignore mi SET cinema_id = c.id FROM cinema c WHERE mi.cinema = c.name;
ALTER TABLE movie_ignore ALTER COLUMN cinema_id SET NOT NULL;
ALTER TABLE movie_ignore DROP COLUMN cinema;
ALTER TABLE movie_ignore ADD PRIMARY KEY (user_id, cinema_id, title);
