use application::{
    AppResult, AuditRepository, Category, LibraryQuery, MediaRepository, PlayLogStart, ProbedMedia,
};
use domain::{AssetKind, CuePoints, MediaAsset};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::{Path, PathBuf};

pub mod migrations;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("migration: {0}")]
    Migration(String),
}

/// SQLite-backed repository. A single connection behind a mutex implements
/// the "single writer" rule; long-running reads should use a separate pool in
/// the server build.
pub struct Database {
    connection: Mutex<Connection>,
    path: PathBuf,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| DbError::Migration(format!("create data dir: {error}")))?;
        }
        let mut connection = Connection::open(&path)?;
        Self::configure(&connection)?;
        migrations::run(&mut connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
            path,
        })
    }

    pub fn open_in_memory() -> Result<Self, DbError> {
        let mut connection = Connection::open_in_memory()?;
        Self::configure(&connection)?;
        migrations::run(&mut connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
            path: PathBuf::from(":memory:"),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Refreshes the query-planner statistics (`ANALYZE`). Call it after a bulk
    /// import or a large delete: without stats SQLite may prefer a full sort
    /// over the partial index that serves the unfiltered library list.
    pub fn analyze(&self) -> AppResult<()> {
        self.connection
            .lock()
            .execute_batch("ANALYZE")
            .map_err(repo_error)
    }

    fn configure(connection: &Connection) -> Result<(), DbError> {
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;
             PRAGMA temp_store = MEMORY;
             PRAGMA cache_size = -64000;
             PRAGMA busy_timeout = 5000;
             PRAGMA wal_autocheckpoint = 1000;",
        )?;
        // In-memory databases report "memory" for WAL; that is expected.
        let _ = connection.pragma_update(None, "journal_mode", "WAL");
        Ok(())
    }

    pub fn station_name(&self) -> AppResult<String> {
        let connection = self.connection.lock();
        connection
            .query_row(
                "SELECT name FROM stations WHERE is_active = 1 LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(repo_error)
            .map(|name| name.unwrap_or_else(|| "Radio Master FM".into()))
    }

    pub fn cart_slots(&self, page_number: i64) -> AppResult<Vec<CartSlot>> {
        let connection = self.connection.lock();
        let mut statement = connection
            .prepare(
                "SELECT cs.id, cs.slot_index, cs.hotkey, cs.color_hex, cs.is_loop, cs.gain_db,
                        ma.id, ma.title, ma.artist, ma.duration_ms
                 FROM cartwall_slots cs
                 LEFT JOIN cartwall_pages cp ON cp.id = cs.page_id
                 LEFT JOIN media_assets ma ON ma.id = cs.asset_id
                 WHERE cp.page_number = ?
                 ORDER BY cs.slot_index",
            )
            .map_err(repo_error)?;

        let rows = statement
            .query_map([page_number], |row| {
                Ok(CartSlot {
                    id: row.get(0)?,
                    slot_index: row.get(1)?,
                    hotkey: row.get(2)?,
                    color_hex: row.get(3)?,
                    is_loop: row.get::<_, i64>(4)? != 0,
                    gain_db: row.get(5)?,
                    asset_id: row.get(6)?,
                    title: row.get(7)?,
                    artist: row.get(8)?,
                    duration_ms: row.get(9)?,
                })
            })
            .map_err(repo_error)?;

        rows.collect::<Result<Vec<_>, _>>().map_err(repo_error)
    }

    /// Looks up a category code by id (used by the audit log).
    pub fn category_code(&self, category_id: i64) -> AppResult<String> {
        let connection = self.connection.lock();
        connection
            .query_row(
                "SELECT code FROM categories WHERE id = ?1",
                [category_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(repo_error)
            .map(|code| code.unwrap_or_else(|| "MUSIC".to_string()))
    }

    /// Number of playout audit rows. Used by tests and reports.
    pub fn playout_logs_count(&self) -> AppResult<i64> {
        let connection = self.connection.lock();
        connection
            .query_row("SELECT COUNT(*) FROM playout_logs", [], |row| row.get(0))
            .map_err(repo_error)
    }

    pub fn save_snapshot(&self, station_id: i64, state_json: &str, clean: bool) -> AppResult<()> {
        let connection = self.connection.lock();
        connection
            .execute(
                "INSERT INTO playout_snapshots (station_id, at_utc_ms, state_json, is_clean_shutdown)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    station_id,
                    chrono::Utc::now().timestamp_millis(),
                    state_json,
                    clean as i64,
                ],
            )
            .map_err(repo_error)?;
        Ok(())
    }

    pub fn latest_snapshot(&self, station_id: i64) -> AppResult<Option<(String, bool, i64)>> {
        let connection = self.connection.lock();
        connection
            .query_row(
                "SELECT state_json, is_clean_shutdown, at_utc_ms
                   FROM playout_snapshots
                  WHERE station_id = ?1
                  ORDER BY id DESC
                  LIMIT 1",
                [station_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)? != 0,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(repo_error)
    }
}

#[derive(Debug, Clone)]
pub struct CartSlot {
    pub id: i64,
    pub slot_index: i64,
    pub hotkey: String,
    pub color_hex: String,
    pub is_loop: bool,
    pub gain_db: f64,
    pub asset_id: Option<i64>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub duration_ms: Option<i64>,
}

fn repo_error(error: rusqlite::Error) -> application::AppError {
    application::AppError::Repository(error.to_string())
}

fn row_to_asset(row: &Row<'_>) -> rusqlite::Result<MediaAsset> {
    let kind: String = row.get("kind")?;
    Ok(MediaAsset {
        id: row.get("id")?,
        category_id: row.get("category_id")?,
        title: row.get("title")?,
        artist: row.get("artist")?,
        album: row.get("album")?,
        release_year: row.get("release_year")?,
        bpm: row.get::<_, i64>("bpm")? as u32,
        kind: if kind == "VIDEO" {
            AssetKind::Video
        } else {
            AssetKind::Audio
        },
        file_path: row.get("file_path")?,
        checksum_sha256: row.get("checksum_sha256")?,
        duration_ms: row.get::<_, i64>("duration_ms")? as u32,
        gain_db: row.get("gain_db")?,
        loudness_lufs: row.get("loudness_lufs")?,
        cue_points: CuePoints {
            cue_in_ms: row.get::<_, i64>("cue_in_ms")? as u32,
            intro_ms: row.get::<_, i64>("intro_ms")? as u32,
            hook_ms: row.get::<_, i64>("hook_ms")? as u32,
            mix_point_ms: row.get::<_, i64>("mix_point_ms")? as u32,
            fade_out_ms: row.get::<_, i64>("fade_out_ms")? as u32,
            cue_out_ms: row.get::<_, i64>("cue_out_ms")? as u32,
        },
        play_count: row.get::<_, i64>("play_count")? as u64,
        last_played_at: row.get("last_played_at")?,
    })
}

const ASSET_COLUMNS: &str = "m.id, m.category_id, m.title, m.artist, m.album, m.release_year,
    m.bpm, m.kind, m.file_path, m.checksum_sha256, m.duration_ms, m.gain_db, m.loudness_lufs,
    m.cue_in_ms, m.intro_ms, m.hook_ms, m.mix_point_ms, m.fade_out_ms, m.cue_out_ms,
    m.play_count, m.last_played_at";

impl MediaRepository for Database {
    fn insert(
        &self,
        category_id: i64,
        probed: &ProbedMedia,
        file_path: &Path,
    ) -> AppResult<MediaAsset> {
        let now = chrono::Utc::now().timestamp_millis();
        let cue = &probed.cue_points;
        let connection = self.connection.lock();
        connection
            .execute(
                "INSERT INTO media_assets (
                    category_id, title, artist, album, release_year, bpm, kind, file_path,
                    checksum_sha256, duration_ms, gain_db, loudness_lufs, true_peak_dbtp,
                    cue_in_ms, intro_ms, hook_ms, mix_point_ms, fade_out_ms, cue_out_ms,
                    created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0.0, ?, -1.0, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    category_id,
                    probed.title,
                    probed.artist,
                    probed.album,
                    probed.release_year,
                    probed.bpm,
                    match probed.kind {
                        AssetKind::Audio => "AUDIO",
                        AssetKind::Video => "VIDEO",
                    },
                    file_path.to_string_lossy(),
                    probed.checksum_sha256,
                    probed.duration_ms,
                    probed.loudness_lufs,
                    cue.cue_in_ms,
                    cue.intro_ms,
                    cue.hook_ms,
                    cue.mix_point_ms,
                    cue.fade_out_ms,
                    cue.cue_out_ms,
                    now,
                    now
                ],
            )
            .map_err(repo_error)?;

        let id = connection.last_insert_rowid();
        drop(connection);
        self.get(id)?
            .ok_or_else(|| application::AppError::Repository("insert returned no row".into()))
    }

    fn find_by_checksum(&self, checksum: &str) -> AppResult<Option<MediaAsset>> {
        let connection = self.connection.lock();
        connection
            .query_row(
                &format!(
                    "SELECT {ASSET_COLUMNS} FROM media_assets m
                     WHERE m.checksum_sha256 = ? AND m.deleted_at IS NULL
                     LIMIT 1"
                ),
                [checksum],
                row_to_asset,
            )
            .optional()
            .map_err(repo_error)
    }

    fn get(&self, id: i64) -> AppResult<Option<MediaAsset>> {
        let connection = self.connection.lock();
        connection
            .query_row(
                &format!(
                    "SELECT {ASSET_COLUMNS} FROM media_assets m
                     WHERE m.id = ? AND m.deleted_at IS NULL"
                ),
                [id],
                row_to_asset,
            )
            .optional()
            .map_err(repo_error)
    }

    fn search(&self, query: &LibraryQuery) -> AppResult<Vec<MediaAsset>> {
        let connection = self.connection.lock();
        let text = query.text.trim();
        let limit = query.limit.clamp(1, 200);
        let offset = query.offset;

        if text.is_empty() {
            // Two statements instead of `?1 IS NULL OR category_id = ?1`: the
            // OR form stops SQLite from using the (category, artist, title)
            // index, which turns the list into a 50k-row sort (spike C).
            let (sql, category) = match query.category_id {
                Some(category_id) => (
                    format!(
                        "SELECT {ASSET_COLUMNS} FROM media_assets m
                         WHERE m.deleted_at IS NULL AND m.is_active = 1
                           AND m.category_id = ?1
                         ORDER BY m.artist COLLATE NOCASE, m.title COLLATE NOCASE
                         LIMIT ?2 OFFSET ?3"
                    ),
                    Some(category_id),
                ),
                None => (
                    format!(
                        "SELECT {ASSET_COLUMNS} FROM media_assets m
                         WHERE m.deleted_at IS NULL AND m.is_active = 1
                         ORDER BY m.artist COLLATE NOCASE, m.title COLLATE NOCASE
                         LIMIT ?1 OFFSET ?2"
                    ),
                    None,
                ),
            };
            let mut statement = connection.prepare(&sql).map_err(repo_error)?;
            let rows = match category {
                Some(category_id) => {
                    statement.query_map(params![category_id, limit, offset], row_to_asset)
                }
                None => statement.query_map(params![limit, offset], row_to_asset),
            }
            .map_err(repo_error)?;
            return rows.collect::<Result<Vec<_>, _>>().map_err(repo_error);
        }

        let fts_query = to_fts_query(text);
        if fts_query == "\"\"" {
            return Ok(Vec::new());
        }
        let mut statement = connection
            .prepare(&format!(
                "SELECT {ASSET_COLUMNS} FROM media_assets m
                 JOIN media_assets_fts f ON f.rowid = m.id
                 WHERE media_assets_fts MATCH ?1
                   AND m.deleted_at IS NULL AND m.is_active = 1
                   AND (?2 IS NULL OR m.category_id = ?2)
                 ORDER BY bm25(media_assets_fts), m.artist COLLATE NOCASE
                 LIMIT ?3 OFFSET ?4"
            ))
            .map_err(repo_error)?;

        let rows = statement
            .query_map(
                params![fts_query, query.category_id, limit, offset],
                row_to_asset,
            )
            .map_err(repo_error)?;

        rows.collect::<Result<Vec<_>, _>>().map_err(repo_error)
    }

    fn categories(&self) -> AppResult<Vec<Category>> {
        let connection = self.connection.lock();
        let mut statement = connection
            .prepare(
                "SELECT id, code, name, color_hex, is_commercial
                 FROM categories ORDER BY id",
            )
            .map_err(repo_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(Category {
                    id: row.get(0)?,
                    code: row.get(1)?,
                    name: row.get(2)?,
                    color_hex: row.get(3)?,
                    is_commercial: row.get::<_, i64>(4)? != 0,
                })
            })
            .map_err(repo_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(repo_error)
    }
}

/// Converts user input into a safe FTS5 prefix query.
/// Tokens are restricted to alphanumeric characters to avoid FTS operators.
pub fn to_fts_query(input: &str) -> String {
    let tokens: Vec<String> = input
        .split_whitespace()
        .filter_map(|token| {
            let cleaned: String = token
                .chars()
                .filter(|character| character.is_alphanumeric())
                .collect();
            if cleaned.is_empty() {
                None
            } else {
                Some(format!("{cleaned}*"))
            }
        })
        .collect();

    if tokens.is_empty() {
        "\"\"".into()
    } else {
        tokens.join(" ")
    }
}

impl AuditRepository for Database {
    fn start_playback(&self, entry: &PlayLogStart) -> AppResult<i64> {
        let connection = self.connection.lock();
        connection
            .execute(
                "INSERT INTO playout_logs (
                    station_id, asset_id, raw_title, raw_artist, category_code,
                    origin, started_at, played_at, loudness_lufs
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8)",
                params![
                    entry.station_id,
                    entry.asset_id,
                    entry.raw_title,
                    entry.raw_artist,
                    entry.category_code,
                    entry.origin,
                    entry.started_at,
                    entry.loudness_lufs,
                ],
            )
            .map_err(repo_error)?;
        Ok(connection.last_insert_rowid())
    }

    fn end_playback(
        &self,
        id: i64,
        ended_at: i64,
        duration_played_ms: u64,
        ended_reason: Option<&str>,
        interrupted: bool,
    ) -> AppResult<()> {
        let connection = self.connection.lock();
        connection
            .execute(
                "UPDATE playout_logs
                    SET ended_at = ?1,
                        duration_played_ms = ?2,
                        ended_reason = ?3,
                        was_interrupted = ?4
                  WHERE id = ?5",
                params![
                    ended_at,
                    duration_played_ms as i64,
                    ended_reason,
                    interrupted as i64,
                    id,
                ],
            )
            .map_err(repo_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probed(checksum: &str, title: &str, artist: &str) -> ProbedMedia {
        ProbedMedia {
            title: title.into(),
            artist: artist.into(),
            album: Some("Album".into()),
            release_year: Some(2024),
            bpm: 120,
            kind: AssetKind::Audio,
            duration_ms: 180_000,
            checksum_sha256: checksum.into(),
            loudness_lufs: -23.0,
            cue_points: CuePoints::whole_file(180_000),
        }
    }

    #[test]
    fn migrates_and_seeds() {
        let db = Database::open_in_memory().unwrap();
        let categories = MediaRepository::categories(&db).unwrap();
        let codes: Vec<&str> = categories.iter().map(|c| c.code.as_str()).collect();
        assert_eq!(
            codes,
            vec!["MUSIC", "JINGLE", "AD", "WHATSAPP", "INBOX", "NEWS"]
        );
        assert_eq!(db.station_name().unwrap(), "Radio Master FM");

        // Cart wall: three pages of eight pads, all empty on a fresh database.
        let slots = db.cart_slots(1).unwrap();
        assert_eq!(slots.len(), 8);
        assert!(slots.iter().all(|slot| slot.asset_id.is_none()));
        assert_eq!(slots[0].hotkey, "F1");
        assert_eq!(slots[7].hotkey, "F8");
    }

    #[test]
    fn inserts_finds_and_searches() {
        let db = Database::open_in_memory().unwrap();
        let path = Path::new("C:/media/Hello World.mp3");
        let asset = db
            .insert(1, &probed("abc", "Hello World", "Daft Punk"), path)
            .unwrap();

        assert_eq!(asset.id, 1);
        assert_eq!(asset.title, "Hello World");
        assert_eq!(
            db.find_by_checksum("abc").unwrap().map(|item| item.id),
            Some(1)
        );

        let results = db
            .search(&LibraryQuery {
                text: "hello".into(),
                limit: 50,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(results.len(), 1);

        let results = db
            .search(&LibraryQuery {
                text: "daft".into(),
                limit: 50,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(results.len(), 1);

        let results = db
            .search(&LibraryQuery {
                text: "zzz".into(),
                limit: 50,
                ..Default::default()
            })
            .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn filters_by_category() {
        let db = Database::open_in_memory().unwrap();
        db.insert(1, &probed("a", "Song", "Artist"), Path::new("a.mp3"))
            .unwrap();
        db.insert(3, &probed("b", "Spot", "Advertiser"), Path::new("b.mp3"))
            .unwrap();

        let music = db
            .search(&LibraryQuery {
                text: String::new(),
                category_id: Some(1),
                limit: 50,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(music.len(), 1);
        assert_eq!(music[0].title, "Song");

        let ads = db
            .search(&LibraryQuery {
                text: String::new(),
                category_id: Some(3),
                limit: 50,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(ads.len(), 1);
        assert_eq!(ads[0].title, "Spot");
    }

    #[test]
    fn fts_syncs_on_update_and_delete() {
        let db = Database::open_in_memory().unwrap();
        let asset = db
            .insert(1, &probed("x", "Original", "Artist"), Path::new("x.mp3"))
            .unwrap();

        {
            let connection = db.connection.lock();
            connection
                .execute(
                    "UPDATE media_assets SET title = 'Renamed' WHERE id = ?",
                    [asset.id],
                )
                .unwrap();
        }

        let old = db
            .search(&LibraryQuery {
                text: "original".into(),
                limit: 50,
                ..Default::default()
            })
            .unwrap();
        assert!(old.is_empty());

        let new = db
            .search(&LibraryQuery {
                text: "renamed".into(),
                limit: 50,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(new.len(), 1);

        {
            let connection = db.connection.lock();
            connection
                .execute("DELETE FROM media_assets WHERE id = ?", [asset.id])
                .unwrap();
        }

        let deleted = db
            .search(&LibraryQuery {
                text: "renamed".into(),
                limit: 50,
                ..Default::default()
            })
            .unwrap();
        assert!(deleted.is_empty());
    }

    #[test]
    fn sanitizes_fts_input() {
        assert_eq!(to_fts_query("AC/DC"), "ACDC*");
        assert_eq!(to_fts_query("  daft   punk "), "daft* punk*");
        assert_eq!(to_fts_query("\"\"\";;"), "\"\"");
    }

    #[test]
    fn audit_start_and_end() {
        use application::{AuditRepository, PlayLogStart};
        let db = Database::open_in_memory().unwrap();
        let id = db
            .start_playback(&PlayLogStart {
                station_id: 1,
                asset_id: None,
                raw_title: "Hello".into(),
                raw_artist: "Artist".into(),
                category_code: "MUSIC".into(),
                origin: "MANUAL",
                started_at: 1000,
                loudness_lufs: Some(-23.0),
            })
            .unwrap();
        assert!(id > 0);
        db.end_playback(id, 2000, 1000, Some("COMPLETED"), false)
            .unwrap();

        assert_eq!(db.playout_logs_count().unwrap(), 1);
    }
}
