-- Where a sound came from, and what its licence obliges.
--
-- Sounds imported from Freesound are mostly Creative Commons, and CC-BY requires the
-- author to be credited wherever the sound is heard. That obligation outlives the
-- import, so it is stored with the sound rather than shown once and forgotten.
--
-- Locally imported files keep `source = 'local'` and carry no licence, which is the
-- correct answer for a file the user already owns: the application knows nothing about
-- its terms and must not invent any.

ALTER TABLE sounds ADD COLUMN source        TEXT NOT NULL DEFAULT 'local';
ALTER TABLE sounds ADD COLUMN source_id     TEXT NOT NULL DEFAULT '';
ALTER TABLE sounds ADD COLUMN source_url    TEXT NOT NULL DEFAULT '';
ALTER TABLE sounds ADD COLUMN license       TEXT NOT NULL DEFAULT '';
ALTER TABLE sounds ADD COLUMN author        TEXT NOT NULL DEFAULT '';
ALTER TABLE sounds ADD COLUMN attribution   TEXT NOT NULL DEFAULT '';

-- Re-importing the same Freesound sound must update the existing row rather than add a
-- second copy. Partial: local files all share the empty source_id.
CREATE UNIQUE INDEX sounds_source_unique
    ON sounds (source, source_id)
    WHERE source_id <> '';
