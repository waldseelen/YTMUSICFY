CREATE TABLE IF NOT EXISTS track_cache (
    video_id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    artists_json TEXT NOT NULL,
    album TEXT,
    duration_sec INTEGER,
    release_year INTEGER,
    music_video_type TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS artist_genres (
    artist_name_norm TEXT PRIMARY KEY,
    raw_tags_json TEXT NOT NULL,
    primary_genre TEXT NOT NULL,
    source TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS track_enrichment (
    cache_key TEXT PRIMARY KEY,
    video_id TEXT,
    tags_json TEXT NOT NULL,
    primary_genre TEXT NOT NULL,
    release_year INTEGER,
    bpm REAL,
    source TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS snapshots (
    id TEXT PRIMARY KEY,
    playlist_id TEXT NOT NULL,
    playlist_title TEXT NOT NULL,
    reason TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    item_count INTEGER NOT NULL,
    payload_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_snapshots_playlist_created
    ON snapshots (playlist_id, created_at DESC);

CREATE TABLE IF NOT EXISTS playlist_cache (
    playlist_id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    manifest_json TEXT,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS staged_ops (
    entry_id INTEGER PRIMARY KEY,
    seq INTEGER NOT NULL,
    selected INTEGER NOT NULL DEFAULT 1,
    op_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_staged_ops_seq
    ON staged_ops (seq ASC, entry_id ASC);

CREATE TABLE IF NOT EXISTS playback_session (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    session_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS play_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    video_id TEXT NOT NULL,
    title TEXT NOT NULL,
    artists_json TEXT NOT NULL,
    album TEXT,
    duration_sec INTEGER,
    played_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_play_history_played_at
    ON play_history (played_at DESC);
