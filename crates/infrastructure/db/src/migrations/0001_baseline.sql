-- ============================================================================
-- 0001_baseline.sql
-- Audiomaster — baseline schema (3NF + FTS5 + audit)
--
-- Conventions:
--   * All timestamps are UTC epoch milliseconds (INTEGER).
--   * Enums are TEXT + CHECK for readability and forward-compatible migrations.
--   * Media files are referenced by canonical path + SHA-256, never embedded.
--
-- PRAGMAs live in Database::configure(); SQLite refuses to change journal mode
-- from inside the migration transaction.
-- ============================================================================

-- ----------------------------------------------------------------------------
-- 1. Stations and users
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS stations (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT NOT NULL UNIQUE COLLATE NOCASE,
    dial_frequency TEXT,
    timezone      TEXT NOT NULL DEFAULT 'America/Asuncion',
    is_active     INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS users (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    username            TEXT NOT NULL UNIQUE COLLATE NOCASE,
    display_name        TEXT NOT NULL,
    role                TEXT NOT NULL CHECK (role IN ('ADMIN', 'OPERATOR', 'PROGRAMMER', 'PRODUCER')),
    password_hash       TEXT NOT NULL,
    password_algo       TEXT NOT NULL DEFAULT 'argon2id',
    failed_attempts     INTEGER NOT NULL DEFAULT 0,
    locked_until        INTEGER,
    must_change_password INTEGER NOT NULL DEFAULT 0 CHECK (must_change_password IN (0, 1)),
    last_login_at       INTEGER,
    is_active           INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at          INTEGER NOT NULL
);

-- ----------------------------------------------------------------------------
-- 2. Catalog
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS categories (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    code          TEXT NOT NULL UNIQUE,
    name          TEXT NOT NULL,
    color_hex     TEXT NOT NULL DEFAULT '#3b82f6',
    is_commercial INTEGER NOT NULL DEFAULT 0 CHECK (is_commercial IN (0, 1))
);

CREATE TABLE IF NOT EXISTS media_assets (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    category_id        INTEGER NOT NULL,
    title              TEXT NOT NULL,
    artist             TEXT NOT NULL DEFAULT '',
    album              TEXT,
    release_year       INTEGER,
    bpm                INTEGER NOT NULL DEFAULT 0 CHECK (bpm >= 0),
    kind               TEXT NOT NULL DEFAULT 'AUDIO' CHECK (kind IN ('AUDIO', 'VIDEO')),
    file_path          TEXT NOT NULL,
    checksum_sha256    TEXT NOT NULL,
    duration_ms        INTEGER NOT NULL CHECK (duration_ms > 0),
    gain_db            REAL NOT NULL DEFAULT 0.0,
    loudness_lufs      REAL NOT NULL DEFAULT -23.0,
    true_peak_dbtp     REAL NOT NULL DEFAULT -1.0,
    lra_lu             REAL NOT NULL DEFAULT 0.0,
    normalization_gain_db REAL NOT NULL DEFAULT 0.0,
    cue_in_ms          INTEGER NOT NULL DEFAULT 0 CHECK (cue_in_ms >= 0),
    intro_ms           INTEGER NOT NULL DEFAULT 0 CHECK (intro_ms >= 0),
    hook_ms            INTEGER NOT NULL DEFAULT 0 CHECK (hook_ms >= 0),
    mix_point_ms       INTEGER NOT NULL DEFAULT 0 CHECK (mix_point_ms >= 0),
    fade_out_ms        INTEGER NOT NULL DEFAULT 0 CHECK (fade_out_ms >= 0),
    cue_out_ms         INTEGER NOT NULL DEFAULT 0 CHECK (cue_out_ms >= 0),
    waveform_peaks     BLOB,
    play_count         INTEGER NOT NULL DEFAULT 0 CHECK (play_count >= 0),
    last_played_at     INTEGER,
    is_active          INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    deleted_at         INTEGER,
    created_at         INTEGER NOT NULL,
    updated_at         INTEGER NOT NULL,
    UNIQUE (file_path),
    FOREIGN KEY (category_id) REFERENCES categories (id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_media_assets_cat_artist ON media_assets (category_id, artist);
CREATE INDEX IF NOT EXISTS idx_media_assets_checksum ON media_assets (checksum_sha256);
CREATE INDEX IF NOT EXISTS idx_media_assets_active ON media_assets (is_active, deleted_at);

-- Sorting the library without a text filter is `ORDER BY artist, title`. The
-- partial index matches the search predicate exactly, so SQLite walks it in
-- order and stops after LIMIT rows instead of sorting every active track
-- (spike C). The category index covers the filtered variant.
CREATE INDEX IF NOT EXISTS idx_media_assets_live_artist_title
    ON media_assets (artist COLLATE NOCASE, title COLLATE NOCASE)
    WHERE is_active = 1 AND deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_media_assets_cat_artist_title
    ON media_assets (category_id, artist COLLATE NOCASE, title COLLATE NOCASE);

CREATE TABLE IF NOT EXISTS tags (
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE
);

CREATE TABLE IF NOT EXISTS asset_tags (
    asset_id INTEGER NOT NULL,
    tag_id   INTEGER NOT NULL,
    PRIMARY KEY (asset_id, tag_id),
    FOREIGN KEY (asset_id) REFERENCES media_assets (id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags (id) ON DELETE CASCADE
);

-- ----------------------------------------------------------------------------
-- 3. FTS5 full-text search (contentless, kept in sync by triggers)
-- ----------------------------------------------------------------------------
CREATE VIRTUAL TABLE IF NOT EXISTS media_assets_fts USING fts5(
    title,
    artist,
    album,
    content='',
    tokenize='unicode61 remove_diacritics 2'
);

CREATE TRIGGER IF NOT EXISTS trg_media_assets_ai AFTER INSERT ON media_assets BEGIN
    INSERT INTO media_assets_fts(rowid, title, artist, album)
    VALUES (new.id, new.title, new.artist, COALESCE(new.album, ''));
END;

CREATE TRIGGER IF NOT EXISTS trg_media_assets_ad AFTER DELETE ON media_assets BEGIN
    INSERT INTO media_assets_fts(media_assets_fts, rowid, title, artist, album)
    VALUES ('delete', old.id, old.title, old.artist, COALESCE(old.album, ''));
END;

CREATE TRIGGER IF NOT EXISTS trg_media_assets_au AFTER UPDATE OF title, artist, album ON media_assets BEGIN
    INSERT INTO media_assets_fts(media_assets_fts, rowid, title, artist, album)
    VALUES ('delete', old.id, old.title, old.artist, COALESCE(old.album, ''));
    INSERT INTO media_assets_fts(rowid, title, artist, album)
    VALUES (new.id, new.title, new.artist, COALESCE(new.album, ''));
END;

-- ----------------------------------------------------------------------------
-- 4. Playlists and playout queue
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS playlists (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id        INTEGER NOT NULL,
    name              TEXT NOT NULL,
    scheduled_for     TEXT,
    total_duration_ms INTEGER NOT NULL DEFAULT 0,
    created_at        INTEGER NOT NULL,
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS playlist_items (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    playlist_id          INTEGER NOT NULL,
    item_type            TEXT NOT NULL DEFAULT 'ASSET'
                           CHECK (item_type IN ('ASSET', 'STREAM', 'COMMAND', 'GENERATED', 'RELAY')),
    asset_id             INTEGER,
    stream_url           TEXT,
    command              TEXT,
    position             TEXT NOT NULL,
    transition_type      TEXT NOT NULL DEFAULT 'CROSSFADE'
                           CHECK (transition_type IN ('CROSSFADE', 'CUT', 'FADE_IN')),
    crossfade_duration_ms INTEGER NOT NULL DEFAULT 3500,
    status               TEXT NOT NULL DEFAULT 'PENDING'
                           CHECK (status IN ('PENDING', 'PLAYED', 'SKIPPED')),
    scheduled_air_time   INTEGER,
    FOREIGN KEY (playlist_id) REFERENCES playlists (id) ON DELETE CASCADE,
    FOREIGN KEY (asset_id) REFERENCES media_assets (id) ON DELETE RESTRICT,
    CHECK (
        (item_type = 'ASSET' AND asset_id IS NOT NULL) OR
        (item_type = 'STREAM' AND stream_url IS NOT NULL) OR
        (item_type = 'COMMAND' AND command IS NOT NULL) OR
        item_type IN ('GENERATED', 'RELAY')
    )
);

CREATE INDEX IF NOT EXISTS idx_playlist_items_seq ON playlist_items (playlist_id, position);

-- ----------------------------------------------------------------------------
-- 5. Cart wall / instant play
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS cartwall_pages (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id  INTEGER NOT NULL,
    page_number INTEGER NOT NULL CHECK (page_number BETWEEN 1 AND 10),
    name        TEXT NOT NULL,
    UNIQUE (station_id, page_number),
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS cartwall_slots (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    page_id    INTEGER NOT NULL,
    slot_index INTEGER NOT NULL CHECK (slot_index BETWEEN 1 AND 16),
    hotkey     TEXT NOT NULL,
    asset_id   INTEGER,
    color_hex  TEXT NOT NULL DEFAULT '#8b5cf6',
    is_loop    INTEGER NOT NULL DEFAULT 0 CHECK (is_loop IN (0, 1)),
    gain_db    REAL NOT NULL DEFAULT 0.0,
    UNIQUE (page_id, slot_index),
    UNIQUE (page_id, hotkey),
    FOREIGN KEY (page_id) REFERENCES cartwall_pages (id) ON DELETE CASCADE,
    FOREIGN KEY (asset_id) REFERENCES media_assets (id) ON DELETE SET NULL
);

-- ----------------------------------------------------------------------------
-- 6. Schedule, rotation and dayparts (RF-16..RF-21)
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS dayparts (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id   INTEGER NOT NULL,
    name         TEXT NOT NULL,
    start_minute INTEGER NOT NULL CHECK (start_minute BETWEEN 0 AND 1439),
    end_minute   INTEGER NOT NULL CHECK (end_minute BETWEEN 0 AND 1440),
    days_of_week TEXT NOT NULL DEFAULT '1,2,3,4,5,6,7',
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE,
    CHECK (end_minute > start_minute)
);

CREATE TABLE IF NOT EXISTS schedule_events (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id      INTEGER NOT NULL,
    name            TEXT NOT NULL,
    trigger_kind    TEXT NOT NULL CHECK (trigger_kind IN ('AT', 'RECURRING', 'DTMF', 'MANUAL', 'API')),
    at_utc_ms       INTEGER,
    recurrence_json TEXT,
    dtmf_code       TEXT,
    start_date      TEXT,
    end_date        TEXT,
    daypart_id      INTEGER,
    priority        INTEGER NOT NULL DEFAULT 100,
    is_active       INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE,
    FOREIGN KEY (daypart_id) REFERENCES dayparts (id) ON DELETE SET NULL,
    CHECK (
        (trigger_kind = 'AT' AND at_utc_ms IS NOT NULL) OR
        (trigger_kind = 'RECURRING' AND recurrence_json IS NOT NULL) OR
        (trigger_kind = 'DTMF' AND dtmf_code IS NOT NULL) OR
        trigger_kind IN ('MANUAL', 'API')
    )
);

CREATE TABLE IF NOT EXISTS schedule_actions (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id     INTEGER NOT NULL,
    position     INTEGER NOT NULL,
    action_kind  TEXT NOT NULL CHECK (action_kind IN
                   ('PLAY_ASSET', 'PLAY_PLAYLIST', 'PLAY_AD_BREAK', 'INSERT_JINGLE',
                    'RELAY_ON', 'RELAY_OFF', 'STOP', 'FADE', 'SET_MODE', 'RUN_COMMAND', 'HTH')),
    payload_json TEXT NOT NULL DEFAULT '{}',
    transition   TEXT NOT NULL DEFAULT 'CROSSFADE' CHECK (transition IN ('CROSSFADE', 'CUT', 'FADE_IN')),
    UNIQUE (event_id, position),
    FOREIGN KEY (event_id) REFERENCES schedule_events (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS rotation_rules (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id     INTEGER NOT NULL,
    name           TEXT NOT NULL,
    daypart_id     INTEGER,
    target_slots   INTEGER NOT NULL DEFAULT 10,
    energy_profile TEXT,
    is_active      INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE,
    FOREIGN KEY (daypart_id) REFERENCES dayparts (id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS rotation_terms (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    rule_id      INTEGER NOT NULL,
    field        TEXT NOT NULL CHECK (field IN
                   ('CATEGORY', 'GENRE', 'ARTIST', 'DECADE', 'YEAR', 'BPM', 'TAG', 'LANGUAGE', 'MOOD')),
    operator     TEXT NOT NULL CHECK (operator IN ('EQ', 'NE', 'IN', 'BETWEEN', 'CONTAINS')),
    value_json   TEXT NOT NULL,
    weight       REAL NOT NULL DEFAULT 1.0,
    is_exclusion INTEGER NOT NULL DEFAULT 0 CHECK (is_exclusion IN (0, 1)),
    FOREIGN KEY (rule_id) REFERENCES rotation_rules (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS separation_rules (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id  INTEGER NOT NULL,
    scope       TEXT NOT NULL CHECK (scope IN ('ARTIST', 'TITLE', 'ALBUM', 'CATEGORY', 'TAG')),
    min_minutes INTEGER NOT NULL DEFAULT 0,
    min_tracks  INTEGER NOT NULL DEFAULT 0,
    is_active   INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE
);

-- ----------------------------------------------------------------------------
-- 7. Advertising traffic (RF-22, RF-24)
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ad_campaigns (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    advertiser_name TEXT NOT NULL,
    contract_number TEXT,
    start_date     TEXT NOT NULL,
    end_date       TEXT NOT NULL,
    is_active      INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1))
);

CREATE TABLE IF NOT EXISTS ad_spots (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    campaign_id  INTEGER NOT NULL,
    asset_id     INTEGER NOT NULL,
    priority     INTEGER NOT NULL DEFAULT 1,
    weight       REAL NOT NULL DEFAULT 1.0,
    hourly_limit INTEGER NOT NULL DEFAULT 2,
    daily_limit  INTEGER NOT NULL DEFAULT 24,
    is_active    INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    FOREIGN KEY (campaign_id) REFERENCES ad_campaigns (id) ON DELETE CASCADE,
    FOREIGN KEY (asset_id) REFERENCES media_assets (id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS ad_schedules (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    spot_id      INTEGER NOT NULL,
    station_id   INTEGER NOT NULL,
    target_time  TEXT NOT NULL,
    days_of_week TEXT NOT NULL DEFAULT '1,2,3,4,5,6,7',
    start_date   TEXT,
    end_date     TEXT,
    priority     INTEGER NOT NULL DEFAULT 1,
    is_active    INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    FOREIGN KEY (spot_id) REFERENCES ad_spots (id) ON DELETE CASCADE,
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS ad_airings (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    spot_id          INTEGER NOT NULL,
    station_id       INTEGER NOT NULL,
    scheduled_utc_ms INTEGER NOT NULL,
    started_utc_ms   INTEGER,
    ended_utc_ms     INTEGER,
    status           TEXT NOT NULL DEFAULT 'SCHEDULED'
                       CHECK (status IN ('SCHEDULED', 'PLAYED', 'SKIPPED', 'FAILED')),
    playout_log_id   INTEGER,
    UNIQUE (spot_id, station_id, scheduled_utc_ms),
    FOREIGN KEY (spot_id) REFERENCES ad_spots (id) ON DELETE RESTRICT,
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_ad_schedules_time ON ad_schedules (station_id, target_time);
CREATE INDEX IF NOT EXISTS idx_ad_airings_query ON ad_airings (station_id, scheduled_utc_ms, status);

-- ----------------------------------------------------------------------------
-- 8. Streaming, logger, DTMF, relay and ingest
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS stream_targets (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id   INTEGER NOT NULL,
    kind         TEXT NOT NULL CHECK (kind IN ('ICECAST', 'SHOUTCAST', 'HTTP')),
    host         TEXT NOT NULL,
    port         INTEGER NOT NULL,
    mount        TEXT,
    protocol     TEXT NOT NULL DEFAULT 'http' CHECK (protocol IN ('http', 'https')),
    bitrate_kbps INTEGER NOT NULL DEFAULT 128,
    codec        TEXT NOT NULL DEFAULT 'MP3' CHECK (codec IN ('MP3', 'AAC', 'OPUS')),
    secret_ref   TEXT NOT NULL,
    metadata_mode TEXT NOT NULL DEFAULT 'ICY' CHECK (metadata_mode IN ('ICY', 'HTTP', 'XML', 'FTP', 'NONE')),
    is_active    INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    UNIQUE (station_id, kind, host, port, mount),
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS logger_recordings (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id      INTEGER NOT NULL,
    source          TEXT NOT NULL CHECK (source IN ('MASTER', 'AUX', 'MIC', 'RELAY', 'STREAM')),
    file_path       TEXT NOT NULL,
    started_utc_ms  INTEGER NOT NULL,
    ended_utc_ms    INTEGER,
    size_bytes      INTEGER,
    checksum_sha256 TEXT,
    retention_days  INTEGER NOT NULL DEFAULT 30,
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS dtmf_events (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id      INTEGER NOT NULL,
    direction       TEXT NOT NULL CHECK (direction IN ('RX', 'TX')),
    code            TEXT NOT NULL,
    source          TEXT NOT NULL,
    detected_utc_ms INTEGER NOT NULL,
    action_taken    TEXT,
    event_id        INTEGER,
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE,
    FOREIGN KEY (event_id) REFERENCES schedule_events (id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS relay_sources (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id       INTEGER NOT NULL,
    name             TEXT NOT NULL,
    input_kind       TEXT NOT NULL CHECK (input_kind IN ('DEVICE', 'STREAM', 'SATELLITE')),
    config_json      TEXT NOT NULL,
    local_ad_replace INTEGER NOT NULL DEFAULT 0 CHECK (local_ad_replace IN (0, 1)),
    is_active        INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS hot_folders (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id          INTEGER NOT NULL,
    path                TEXT NOT NULL,
    default_category_id INTEGER,
    default_tags        TEXT,
    is_active           INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    UNIQUE (station_id, path),
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE,
    FOREIGN KEY (default_category_id) REFERENCES categories (id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS ingest_jobs (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    hot_folder_id  INTEGER,
    source_kind    TEXT NOT NULL CHECK (source_kind IN ('HOT_FOLDER', 'WHATSAPP', 'MANUAL', 'CD')),
    source_ref     TEXT,
    status         TEXT NOT NULL DEFAULT 'PENDING'
                     CHECK (status IN ('PENDING', 'RUNNING', 'DONE', 'FAILED', 'QUARANTINED')),
    asset_id       INTEGER,
    error_code     TEXT,
    created_utc_ms INTEGER NOT NULL,
    updated_utc_ms INTEGER NOT NULL,
    FOREIGN KEY (hot_folder_id) REFERENCES hot_folders (id) ON DELETE SET NULL,
    FOREIGN KEY (asset_id) REFERENCES media_assets (id) ON DELETE SET NULL
);

-- ----------------------------------------------------------------------------
-- 9. Audit, access and runtime state
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS playout_logs (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id         INTEGER NOT NULL,
    asset_id           INTEGER,
    playlist_item_id   INTEGER,
    event_id           INTEGER,
    ad_spot_id         INTEGER,
    raw_title          TEXT NOT NULL,
    raw_artist         TEXT NOT NULL DEFAULT '',
    category_code      TEXT NOT NULL,
    origin             TEXT NOT NULL CHECK (origin IN ('AUTO', 'MANUAL', 'CARTWALL', 'SCHEDULE', 'RELAY_DTMF')),
    started_at         INTEGER NOT NULL,
    ended_at           INTEGER,
    played_at          INTEGER NOT NULL,
    duration_played_ms INTEGER NOT NULL DEFAULT 0,
    loudness_lufs      REAL,
    true_peak_dbtp     REAL,
    ended_reason       TEXT,
    was_interrupted    INTEGER NOT NULL DEFAULT 0 CHECK (was_interrupted IN (0, 1)),
    operator_username  TEXT,
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE,
    FOREIGN KEY (asset_id) REFERENCES media_assets (id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_playout_logs_query ON playout_logs (station_id, played_at);

CREATE TABLE IF NOT EXISTS user_station_roles (
    user_id          INTEGER NOT NULL,
    station_id       INTEGER NOT NULL,
    role             TEXT NOT NULL CHECK (role IN ('ADMIN', 'OPERATOR', 'PROGRAMMER', 'PRODUCER')),
    permissions_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (user_id, station_id),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS audit_actions (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER,
    station_id   INTEGER,
    action       TEXT NOT NULL,
    entity       TEXT,
    entity_id    TEXT,
    payload_json TEXT,
    at_utc_ms    INTEGER NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE SET NULL,
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS app_settings (
    key            TEXT PRIMARY KEY,
    value_json     TEXT NOT NULL,
    updated_utc_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS playout_snapshots (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id        INTEGER NOT NULL,
    at_utc_ms         INTEGER NOT NULL,
    state_json        TEXT NOT NULL,
    is_clean_shutdown INTEGER NOT NULL DEFAULT 0 CHECK (is_clean_shutdown IN (0, 1)),
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE
);

-- ----------------------------------------------------------------------------
-- 10. Seed data
-- ----------------------------------------------------------------------------
INSERT OR IGNORE INTO stations (id, name, dial_frequency, timezone, is_active, created_at)
VALUES (1, 'Radio Master FM', '98.5', 'America/Asuncion', 1, CAST(strftime('%s', 'now') AS INTEGER) * 1000);

INSERT OR IGNORE INTO categories (id, code, name, color_hex, is_commercial) VALUES
    (1, 'MUSIC',    'Música',     '#3b82f6', 0),
    (2, 'JINGLE',   'Jingles',    '#8b5cf6', 0),
    (3, 'AD',       'Publicidad', '#f97316', 1),
    (4, 'WHATSAPP', 'WhatsApp',   '#10b981', 0),
    (5, 'INBOX',    'Bandeja de entrada', '#64748b', 0),
    (6, 'NEWS',     'Noticias',   '#ef4444', 0);

INSERT OR IGNORE INTO cartwall_pages (id, station_id, page_number, name) VALUES
    (1, 1, 1, 'Página 1'),
    (2, 1, 2, 'Página 2'),
    (3, 1, 3, 'Página 3');

-- Eight pads per page, following the F1..F8 convention used by the UI. Pads
-- start empty: they are assigned from the library during operation.
INSERT OR IGNORE INTO cartwall_slots (page_id, slot_index, hotkey, color_hex, is_loop, gain_db)
VALUES
    (1, 1, 'F1', '#8b5cf6', 0, 0.0),
    (1, 2, 'F2', '#8b5cf6', 0, 0.0),
    (1, 3, 'F3', '#f97316', 0, 0.0),
    (1, 4, 'F4', '#f97316', 0, 0.0),
    (1, 5, 'F5', '#10b981', 0, 0.0),
    (1, 6, 'F6', '#3b82f6', 0, 0.0),
    (1, 7, 'F7', '#3b82f6', 0, 0.0),
    (1, 8, 'F8', '#ef4444', 0, -3.0),
    (2, 1, 'F1', '#8b5cf6', 0, 0.0),
    (2, 2, 'F2', '#8b5cf6', 0, 0.0),
    (2, 3, 'F3', '#f97316', 0, 0.0),
    (2, 4, 'F4', '#f97316', 0, 0.0),
    (2, 5, 'F5', '#10b981', 0, 0.0),
    (2, 6, 'F6', '#3b82f6', 0, 0.0),
    (2, 7, 'F7', '#3b82f6', 0, 0.0),
    (2, 8, 'F8', '#ef4444', 0, -3.0),
    (3, 1, 'F1', '#8b5cf6', 0, 0.0),
    (3, 2, 'F2', '#8b5cf6', 0, 0.0),
    (3, 3, 'F3', '#f97316', 0, 0.0),
    (3, 4, 'F4', '#f97316', 0, 0.0),
    (3, 5, 'F5', '#10b981', 0, 0.0),
    (3, 6, 'F6', '#3b82f6', 0, 0.0),
    (3, 7, 'F7', '#3b82f6', 0, 0.0),
    (3, 8, 'F8', '#ef4444', 0, -3.0);
