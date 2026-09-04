-- Let built-in events keep improving without overwriting the user's edits.
--
-- The seed events live in code, and until now they reached the database exactly once:
-- `seed_if_empty` ran on a fresh install and never again. Every later improvement to the
-- phrasing — a missing verb, a new negative phrase — was invisible to anyone who had
-- already launched the application. That is how "вдарив мечем" stayed unrecognised after
-- the verb was added.
--
-- So each event now records whether it came from the seed and whether a person has since
-- changed it. On startup the seed is re-applied to built-in events nobody has touched,
-- and left strictly alone everywhere else.

ALTER TABLE events ADD COLUMN builtin       INTEGER NOT NULL DEFAULT 0;
ALTER TABLE events ADD COLUMN user_modified INTEGER NOT NULL DEFAULT 0;

-- Everything already in the database at this point was either seeded from code or
-- created by hand in the editor. The ones matching a current built-in id are marked as
-- built-in; anything else stays a user event and is never rewritten.
UPDATE events SET builtin = 1 WHERE id IN (
    'OPEN_DOOR', 'DOOR_SLAM', 'CHEST_OPEN', 'SWORD_SWING', 'BOW_SHOT', 'THUNDER',
    'FIRE', 'WATER_SPLASH', 'WOLF_GROWL', 'SMALL_CREATURE_SCURRY', 'FOOTSTEPS',
    'MAGIC_CAST', 'COINS', 'GLASS_SHATTER'
);
