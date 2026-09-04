-- Metadata only. Audio files always live on disk and are referenced by path;
-- nothing in this schema ever stores sample data.

-- ---------------------------------------------------------------- profiles --
CREATE TABLE profiles (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL UNIQUE,
    description TEXT    NOT NULL DEFAULT '',
    is_active   INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

-- ------------------------------------------------------------------ sounds --
-- `managed` distinguishes the two library modes: 0 = the file is
-- referenced where the user keeps it, 1 = the app copied it into its own directory.
CREATE TABLE sounds (
    id           INTEGER PRIMARY KEY,
    display_name TEXT    NOT NULL,
    file_path    TEXT    NOT NULL UNIQUE,
    managed      INTEGER NOT NULL DEFAULT 0,
    format       TEXT    NOT NULL DEFAULT '',
    duration_ms  INTEGER,
    sample_rate  INTEGER,
    channels     INTEGER,
    volume       REAL    NOT NULL DEFAULT 1.0,
    weight       REAL    NOT NULL DEFAULT 1.0,
    enabled      INTEGER NOT NULL DEFAULT 1,
    favorite     INTEGER NOT NULL DEFAULT 0,
    missing      INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

CREATE TABLE tags (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE sound_tags (
    sound_id INTEGER NOT NULL REFERENCES sounds(id) ON DELETE CASCADE,
    tag_id   INTEGER NOT NULL REFERENCES tags(id)   ON DELETE CASCADE,
    PRIMARY KEY (sound_id, tag_id)
);
CREATE INDEX idx_sound_tags_tag ON sound_tags(tag_id);

-- ------------------------------------------------------------ sound groups --
CREATE TABLE sound_groups (
    id             INTEGER PRIMARY KEY,
    name           TEXT    NOT NULL UNIQUE,
    -- 'random' | 'weighted' | 'sequential'
    selection_mode TEXT    NOT NULL DEFAULT 'random',
    prevent_repeat INTEGER NOT NULL DEFAULT 1,
    volume         REAL    NOT NULL DEFAULT 1.0,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL
);

CREATE TABLE sound_group_members (
    group_id INTEGER NOT NULL REFERENCES sound_groups(id) ON DELETE CASCADE,
    sound_id INTEGER NOT NULL REFERENCES sounds(id)       ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (group_id, sound_id)
);
CREATE INDEX idx_group_members_sound ON sound_group_members(sound_id);

-- ------------------------------------------------------------------ events --
-- `id` is the semantic identifier, e.g. 'OPEN_DOOR'.
-- `kind` separates one-shots from persistent ambience state transitions.
CREATE TABLE events (
    id                   TEXT    PRIMARY KEY,
    display_name         TEXT    NOT NULL,
    category             TEXT    NOT NULL DEFAULT '',
    sound_group_id       INTEGER REFERENCES sound_groups(id) ON DELETE SET NULL,
    -- 'one_shot' | 'ambience_start' | 'ambience_stop'
    kind                 TEXT    NOT NULL DEFAULT 'one_shot',
    -- 'sfx' | 'ambience' | 'music' | 'voice'
    track                TEXT    NOT NULL DEFAULT 'sfx',
    confidence_threshold REAL    NOT NULL DEFAULT 0.82,
    cooldown_ms          INTEGER NOT NULL DEFAULT 3000,
    probability          REAL    NOT NULL DEFAULT 1.0,
    require_action_word  INTEGER NOT NULL DEFAULT 1,
    enabled              INTEGER NOT NULL DEFAULT 1,
    created_at           INTEGER NOT NULL,
    updated_at           INTEGER NOT NULL
);
CREATE INDEX idx_events_category ON events(category);

-- Example phrases, per language. `kind` = 'example' feeds automatic detection;
-- 'command' is an explicit DM voice command with higher priority.
CREATE TABLE event_phrases (
    id         INTEGER PRIMARY KEY,
    event_id   TEXT    NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    lang       TEXT    NOT NULL DEFAULT 'en',
    text       TEXT    NOT NULL,
    kind       TEXT    NOT NULL DEFAULT 'example',
    created_at INTEGER NOT NULL,
    UNIQUE (event_id, lang, text, kind)
);
CREATE INDEX idx_event_phrases_event ON event_phrases(event_id);

-- `kind` = 'keyword' (retrieval), 'negative' (blocklist), 'action' (action gating).
CREATE TABLE event_terms (
    id       INTEGER PRIMARY KEY,
    event_id TEXT    NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    lang     TEXT    NOT NULL DEFAULT 'en',
    text     TEXT    NOT NULL,
    kind     TEXT    NOT NULL,
    UNIQUE (event_id, lang, text, kind)
);
CREATE INDEX idx_event_terms_event ON event_terms(event_id);
CREATE INDEX idx_event_terms_kind  ON event_terms(kind);

-- Event chaining: DRAW_SWORD +0ms then SWORD_SWING +450ms.
CREATE TABLE event_chains (
    id                INTEGER PRIMARY KEY,
    event_id          TEXT    NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    follower_event_id TEXT    NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    delay_ms          INTEGER NOT NULL DEFAULT 0,
    probability       REAL    NOT NULL DEFAULT 1.0,
    UNIQUE (event_id, follower_event_id)
);

-- Precomputed phrase embeddings. Keyed by model + phrase hash so they are
-- recomputed only when the phrase text or the embedding model changes,
-- never on the transcript hot path.
CREATE TABLE phrase_embeddings (
    phrase_id   INTEGER NOT NULL REFERENCES event_phrases(id) ON DELETE CASCADE,
    model_id    TEXT    NOT NULL,
    phrase_hash TEXT    NOT NULL,
    dim         INTEGER NOT NULL,
    vector      BLOB    NOT NULL,
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (phrase_id, model_id)
);

-- Per-profile overrides. A campaign enables a subset of events and can retune them.
CREATE TABLE profile_events (
    profile_id            INTEGER NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    event_id              TEXT    NOT NULL REFERENCES events(id)   ON DELETE CASCADE,
    enabled               INTEGER NOT NULL DEFAULT 1,
    threshold_override    REAL,
    cooldown_override     INTEGER,
    sound_group_override  INTEGER REFERENCES sound_groups(id) ON DELETE SET NULL,
    PRIMARY KEY (profile_id, event_id)
);

-- ---------------------------------------------------------------- settings --
CREATE TABLE settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

-- ------------------------------------------------------------ session logs --
-- Event history only. Raw microphone audio is never recorded here.
CREATE TABLE sessions (
    id         INTEGER PRIMARY KEY,
    profile_id INTEGER REFERENCES profiles(id) ON DELETE SET NULL,
    started_at INTEGER NOT NULL,
    ended_at   INTEGER
);

CREATE TABLE session_events (
    id         INTEGER PRIMARY KEY,
    session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    at         INTEGER NOT NULL,
    event_id   TEXT    NOT NULL,
    confidence REAL    NOT NULL,
    sound_id   INTEGER REFERENCES sounds(id) ON DELETE SET NULL,
    transcript TEXT    NOT NULL DEFAULT ''
);
CREATE INDEX idx_session_events_session ON session_events(session_id, at);
