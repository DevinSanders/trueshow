//! Local-first SQLite storage engine (WAL + FTS5) for OpenPresenter.
//!
//! Guarantees 100% offline Sunday-morning operation across songs, slide presentations,
//! service playlists, Bible verses, BLAKE3 content-addressed media assets, CCLI usage
//! audit logs, and CRDT delta persistence.

use chrono::{DateTime, Utc};
use core_scene::{
    Arrangement, LOOK_AUDIENCE_FULL, LOOK_STAGE_CONFIDENCE, LookConfig, LookId, MessageTemplate,
    OverlayItem, Slide, SlideId, TemplateConfig, TemplateId, default_looks,
    default_message_templates, default_overlays, default_templates,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("SQLite error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub const DEFAULT_LIBRARY_ID: &str = "default";
pub const DEFAULT_MEDIA_BIN_ID: &str = "backgrounds";

fn default_library_id() -> String {
    DEFAULT_LIBRARY_ID.to_string()
}

fn default_media_bin_id() -> String {
    DEFAULT_MEDIA_BIN_ID.to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresentationLibraryRecord {
    pub id: String,
    pub name: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaBinRecord {
    pub id: String,
    pub name: String,
    pub category: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SongRecord {
    pub id: String,
    #[serde(default = "default_library_id")]
    pub library_id: String,
    pub title: String,
    pub author: String,
    pub copyright: String,
    pub ccli_number: Option<String>,
    pub key_signature: Option<String>,
    pub bpm: Option<f32>,
    pub lyrics_plain: String,
    pub slides: Vec<Slide>,
    #[serde(default)]
    pub arrangements: Vec<Arrangement>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistRecord {
    pub id: String,
    pub name: String,
    pub item_song_ids: Vec<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaAssetRecord {
    pub blake3_hash: String,
    #[serde(default = "default_media_bin_id")]
    pub bin_id: String,
    pub file_name: String,
    pub local_path: String,
    pub kind: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u64>,
    pub has_alpha: bool,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BibleVerseRecord {
    pub translation: String,
    pub book: String,
    pub chapter: u32,
    pub verse: u32,
    pub text: String,
    pub attribution: String,
    pub cached_at_epoch: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CcliUsageEntry {
    pub id: i64,
    pub projected_at: String,
    pub service_profile: String,
    pub song_title: String,
    pub ccli_number: String,
    pub author: String,
    pub display_seconds: u32,
}

/// Named multi-screen Look routing snapshot (e.g. "Worship Mode", "Sermon Lower-Third").
/// Maps `output_id` (UUID string) → `look_id` (UUID string).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookPresetRecord {
    pub id: String,
    pub name: String,
    pub assignments: BTreeMap<String, String>,
    pub updated_at: DateTime<Utc>,
}

/// Reusable automation Macro storing ordered command strings (e.g. `"clear:all"`, `"blackout:off"`, `"overlay:show:overlay-speaker-lt"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroRecord {
    pub id: String,
    pub name: String,
    pub steps: Vec<String>,
    pub updated_at: DateTime<Utc>,
}

/// Recurring wall-clock calendar schedule entry triggering a Macro at `trigger_hhmm` (24h `HH:MM`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledEventRecord {
    pub id: String,
    pub name: String,
    pub trigger_hhmm: String,
    pub macro_id: String,
    pub enabled: bool,
    pub updated_at: DateTime<Utc>,
}

/// Authoritative offline SQLite repository.
pub struct LibraryStore {
    conn: Connection,
}

impl LibraryStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, LibraryError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.configure_and_migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self, LibraryError> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.configure_and_migrate()?;
        Ok(store)
    }

    fn configure_and_migrate(&self) -> Result<(), LibraryError> {
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            PRAGMA synchronous = NORMAL;

            CREATE TABLE IF NOT EXISTS presentation_libraries (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS media_bins (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT 'all',
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS songs (
                id TEXT PRIMARY KEY,
                library_id TEXT NOT NULL DEFAULT 'default',
                title TEXT NOT NULL,
                author TEXT NOT NULL DEFAULT '',
                copyright TEXT NOT NULL DEFAULT '',
                ccli_number TEXT,
                key_signature TEXT,
                bpm REAL,
                lyrics_plain TEXT NOT NULL DEFAULT '',
                slides_json TEXT NOT NULL,
                arrangements_json TEXT NOT NULL DEFAULT '[]',
                updated_at TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS songs_fts USING fts5(
                song_id UNINDEXED,
                title,
                author,
                ccli_number,
                lyrics_plain
            );

            CREATE TABLE IF NOT EXISTS playlists (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                items_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS media_assets (
                blake3_hash TEXT PRIMARY KEY,
                bin_id TEXT NOT NULL DEFAULT 'backgrounds',
                file_name TEXT NOT NULL,
                local_path TEXT NOT NULL,
                kind TEXT NOT NULL,
                width INTEGER,
                height INTEGER,
                duration_ms INTEGER,
                has_alpha INTEGER NOT NULL DEFAULT 0,
                size_bytes INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS bible_verses (
                translation TEXT NOT NULL,
                book TEXT NOT NULL,
                chapter INTEGER NOT NULL,
                verse INTEGER NOT NULL,
                text TEXT NOT NULL,
                attribution TEXT NOT NULL DEFAULT '',
                cached_at_epoch INTEGER,
                PRIMARY KEY (translation, book, chapter, verse)
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS bible_verses_fts USING fts5(
                translation UNINDEXED,
                book,
                chapter UNINDEXED,
                verse UNINDEXED,
                text
            );

            CREATE TABLE IF NOT EXISTS ccli_usage_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                projected_at TEXT NOT NULL,
                service_profile TEXT NOT NULL,
                song_title TEXT NOT NULL,
                ccli_number TEXT NOT NULL,
                author TEXT NOT NULL DEFAULT '',
                display_seconds INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS crdt_snapshots (
                doc_id TEXT PRIMARY KEY,
                snapshot_blob BLOB NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS looks (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                config_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS templates (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT 'Custom',
                config_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS look_presets (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                assignments_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS overlays (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                config_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS message_templates (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                config_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS macros (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                steps_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS scheduled_events (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                trigger_hhmm TEXT NOT NULL,
                macro_id TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                updated_at TEXT NOT NULL
            );
            "#,
        )?;
        // Forward-compatible column migrations for existing on-disk SQLite databases.
        let _ = self.conn.execute(
            "ALTER TABLE songs ADD COLUMN library_id TEXT NOT NULL DEFAULT 'default'",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE songs ADD COLUMN arrangements_json TEXT NOT NULL DEFAULT '[]'",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE media_assets ADD COLUMN bin_id TEXT NOT NULL DEFAULT 'backgrounds'",
            [],
        );
        Ok(())
    }

    pub fn upsert_library(&self, lib: &PresentationLibraryRecord) -> Result<(), LibraryError> {
        self.conn.execute(
            r#"
            INSERT INTO presentation_libraries (id, name, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                updated_at = excluded.updated_at
            "#,
            params![lib.id, lib.name, lib.updated_at.to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn list_libraries(&self) -> Result<Vec<PresentationLibraryRecord>, LibraryError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, updated_at FROM presentation_libraries ORDER BY name COLLATE NOCASE ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let updated_str: String = row.get(2)?;
            Ok(PresentationLibraryRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                updated_at: DateTime::parse_from_rfc3339(&updated_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn delete_library(&self, id: &str) -> Result<bool, LibraryError> {
        if id == DEFAULT_LIBRARY_ID {
            return Ok(false);
        }
        self.conn.execute(
            "UPDATE songs SET library_id = ?1 WHERE library_id = ?2",
            params![DEFAULT_LIBRARY_ID, id],
        )?;
        let removed = self
            .conn
            .execute("DELETE FROM presentation_libraries WHERE id = ?1", params![id])?;
        Ok(removed > 0)
    }

    pub fn upsert_song(&self, song: &SongRecord) -> Result<(), LibraryError> {
        let slides_json = serde_json::to_string(&song.slides)?;
        let arrangements_json = serde_json::to_string(&song.arrangements)?;
        let lib_id = if song.library_id.trim().is_empty() {
            DEFAULT_LIBRARY_ID
        } else {
            song.library_id.as_str()
        };
        self.conn.execute(
            r#"
            INSERT INTO songs (id, library_id, title, author, copyright, ccli_number, key_signature, bpm, lyrics_plain, slides_json, arrangements_json, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(id) DO UPDATE SET
                library_id = excluded.library_id,
                title = excluded.title,
                author = excluded.author,
                copyright = excluded.copyright,
                ccli_number = excluded.ccli_number,
                key_signature = excluded.key_signature,
                bpm = excluded.bpm,
                lyrics_plain = excluded.lyrics_plain,
                slides_json = excluded.slides_json,
                arrangements_json = excluded.arrangements_json,
                updated_at = excluded.updated_at
            "#,
            params![
                song.id,
                lib_id,
                song.title,
                song.author,
                song.copyright,
                song.ccli_number,
                song.key_signature,
                song.bpm,
                song.lyrics_plain,
                slides_json,
                arrangements_json,
                song.updated_at.to_rfc3339(),
            ],
        )?;

        self.conn
            .execute("DELETE FROM songs_fts WHERE song_id = ?1", params![song.id])?;
        self.conn.execute(
            "INSERT INTO songs_fts (song_id, title, author, ccli_number, lyrics_plain) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                song.id,
                song.title,
                song.author,
                song.ccli_number.as_deref().unwrap_or(""),
                song.lyrics_plain,
            ],
        )?;
        Ok(())
    }

    pub fn delete_song(&self, id: &str) -> Result<bool, LibraryError> {
        self.conn
            .execute("DELETE FROM songs_fts WHERE song_id = ?1", params![id])?;
        let removed = self
            .conn
            .execute("DELETE FROM songs WHERE id = ?1", params![id])?;
        Ok(removed > 0)
    }

    pub fn move_song_to_library(
        &self,
        song_id: &str,
        target_library_id: &str,
    ) -> Result<bool, LibraryError> {
        let lib_id = if target_library_id.trim().is_empty() || target_library_id == "all" {
            DEFAULT_LIBRARY_ID
        } else {
            target_library_id
        };
        let updated = self.conn.execute(
            "UPDATE songs SET library_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![lib_id, Utc::now().to_rfc3339(), song_id],
        )?;
        Ok(updated > 0)
    }

    pub fn list_songs(&self) -> Result<Vec<SongRecord>, LibraryError> {
        self.list_songs_in_library(None)
    }

    pub fn list_songs_in_library(
        &self,
        library_id: Option<&str>,
    ) -> Result<Vec<SongRecord>, LibraryError> {
        let mut out = Vec::new();
        if let Some(lib_id) = library_id {
            let mut stmt = self.conn.prepare(
                "SELECT id, library_id, title, author, copyright, ccli_number, key_signature, bpm, lyrics_plain, slides_json, arrangements_json, updated_at FROM songs WHERE library_id = ?1 ORDER BY title COLLATE NOCASE ASC",
            )?;
            let rows = stmt.query_map(params![lib_id], row_to_song)?;
            for row in rows {
                out.push(row?);
            }
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, library_id, title, author, copyright, ccli_number, key_signature, bpm, lyrics_plain, slides_json, arrangements_json, updated_at FROM songs ORDER BY title COLLATE NOCASE ASC",
            )?;
            let rows = stmt.query_map([], row_to_song)?;
            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    }

    pub fn get_song(&self, id: &str) -> Result<Option<SongRecord>, LibraryError> {
        self.conn
            .query_row(
                "SELECT id, library_id, title, author, copyright, ccli_number, key_signature, bpm, lyrics_plain, slides_json, arrangements_json, updated_at FROM songs WHERE id = ?1",
                params![id],
                row_to_song,
            )
            .optional()
            .map_err(LibraryError::from)
    }

    /// Full-text search across song titles, authors, CCLI numbers, and lyrics using SQLite FTS5.
    pub fn search_songs(&self, query: &str, limit: usize) -> Result<Vec<SongRecord>, LibraryError> {
        self.search_songs_in_library(None, query, limit)
    }

    pub fn search_songs_in_library(
        &self,
        library_id: Option<&str>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SongRecord>, LibraryError> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return self.list_songs_in_library(library_id);
        }
        let fts_query = sanitize_fts_query(trimmed);
        if fts_query.is_empty() {
            return self.list_songs_in_library(library_id);
        }
        let mut out = Vec::new();
        if let Some(lib_id) = library_id {
            let mut stmt = self.conn.prepare(
                r#"
                SELECT s.id, s.library_id, s.title, s.author, s.copyright, s.ccli_number, s.key_signature, s.bpm, s.lyrics_plain, s.slides_json, s.arrangements_json, s.updated_at
                FROM songs_fts f
                JOIN songs s ON s.id = f.song_id
                WHERE songs_fts MATCH ?1 AND s.library_id = ?2
                ORDER BY rank
                LIMIT ?3
                "#,
            )?;
            let rows = stmt.query_map(params![fts_query, lib_id, limit as i64], row_to_song)?;
            for row in rows {
                out.push(row?);
            }
        } else {
            let mut stmt = self.conn.prepare(
                r#"
                SELECT s.id, s.library_id, s.title, s.author, s.copyright, s.ccli_number, s.key_signature, s.bpm, s.lyrics_plain, s.slides_json, s.arrangements_json, s.updated_at
                FROM songs_fts f
                JOIN songs s ON s.id = f.song_id
                WHERE songs_fts MATCH ?1
                ORDER BY rank
                LIMIT ?2
                "#,
            )?;
            let rows = stmt.query_map(params![fts_query, limit as i64], row_to_song)?;
            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    }

    pub fn upsert_playlist(&self, playlist: &PlaylistRecord) -> Result<(), LibraryError> {
        let items_json = serde_json::to_string(&playlist.item_song_ids)?;
        self.conn.execute(
            r#"
            INSERT INTO playlists (id, name, items_json, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                items_json = excluded.items_json,
                updated_at = excluded.updated_at
            "#,
            params![
                playlist.id,
                playlist.name,
                items_json,
                playlist.updated_at.to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn delete_playlist(&self, id: &str) -> Result<bool, LibraryError> {
        let removed = self
            .conn
            .execute("DELETE FROM playlists WHERE id = ?1", params![id])?;
        Ok(removed > 0)
    }

    pub fn list_playlists(&self) -> Result<Vec<PlaylistRecord>, LibraryError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, items_json, updated_at FROM playlists ORDER BY updated_at DESC")?;
        let rows = stmt.query_map([], |row| {
            let items_json: String = row.get(2)?;
            let updated_str: String = row.get(3)?;
            Ok(PlaylistRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                item_song_ids: serde_json::from_str(&items_json).unwrap_or_default(),
                updated_at: DateTime::parse_from_rfc3339(&updated_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn upsert_media_bin(&self, bin: &MediaBinRecord) -> Result<(), LibraryError> {
        self.conn.execute(
            r#"
            INSERT INTO media_bins (id, name, category, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                category = excluded.category,
                updated_at = excluded.updated_at
            "#,
            params![bin.id, bin.name, bin.category, bin.updated_at.to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn list_media_bins(&self) -> Result<Vec<MediaBinRecord>, LibraryError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, category, updated_at FROM media_bins ORDER BY name COLLATE NOCASE ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let updated_str: String = row.get(3)?;
            Ok(MediaBinRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                category: row.get(2)?,
                updated_at: DateTime::parse_from_rfc3339(&updated_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn delete_media_bin(&self, id: &str) -> Result<bool, LibraryError> {
        if id == DEFAULT_MEDIA_BIN_ID {
            return Ok(false);
        }
        self.conn.execute(
            "UPDATE media_assets SET bin_id = ?1 WHERE bin_id = ?2",
            params![DEFAULT_MEDIA_BIN_ID, id],
        )?;
        let removed = self
            .conn
            .execute("DELETE FROM media_bins WHERE id = ?1", params![id])?;
        Ok(removed > 0)
    }

    /// Computes BLAKE3 hash of a local media file and registers it in the content-addressed manifest.
    pub fn register_media_file(
        &self,
        path: impl AsRef<Path>,
        kind: &str,
        width: Option<u32>,
        height: Option<u32>,
        duration_ms: Option<u64>,
        has_alpha: bool,
    ) -> Result<MediaAssetRecord, LibraryError> {
        self.register_media_file_in_bin(
            path,
            DEFAULT_MEDIA_BIN_ID,
            kind,
            width,
            height,
            duration_ms,
            has_alpha,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn register_media_file_in_bin(
        &self,
        path: impl AsRef<Path>,
        bin_id: &str,
        kind: &str,
        width: Option<u32>,
        height: Option<u32>,
        duration_ms: Option<u64>,
        has_alpha: bool,
    ) -> Result<MediaAssetRecord, LibraryError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)?;
        let hash = blake3::hash(&bytes).to_hex().to_string();
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unnamed".to_string());

        let record = MediaAssetRecord {
            blake3_hash: hash,
            bin_id: if bin_id.trim().is_empty() {
                DEFAULT_MEDIA_BIN_ID.to_string()
            } else {
                bin_id.to_string()
            },
            file_name,
            local_path: path.to_string_lossy().into_owned(),
            kind: kind.to_string(),
            width,
            height,
            duration_ms,
            has_alpha,
            size_bytes: bytes.len() as u64,
        };
        self.upsert_media_asset(&record)?;
        Ok(record)
    }

    pub fn upsert_media_asset(&self, asset: &MediaAssetRecord) -> Result<(), LibraryError> {
        let bin_id = if asset.bin_id.trim().is_empty() {
            DEFAULT_MEDIA_BIN_ID
        } else {
            asset.bin_id.as_str()
        };
        self.conn.execute(
            r#"
            INSERT INTO media_assets (blake3_hash, bin_id, file_name, local_path, kind, width, height, duration_ms, has_alpha, size_bytes)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(blake3_hash) DO UPDATE SET
                bin_id = excluded.bin_id,
                file_name = excluded.file_name,
                local_path = excluded.local_path,
                kind = excluded.kind,
                width = excluded.width,
                height = excluded.height,
                duration_ms = excluded.duration_ms,
                has_alpha = excluded.has_alpha,
                size_bytes = excluded.size_bytes
            "#,
            params![
                asset.blake3_hash,
                bin_id,
                asset.file_name,
                asset.local_path,
                asset.kind,
                asset.width,
                asset.height,
                asset.duration_ms,
                i32::from(asset.has_alpha),
                asset.size_bytes as i64,
            ],
        )?;
        Ok(())
    }

    pub fn delete_media_asset(&self, blake3_hash: &str) -> Result<bool, LibraryError> {
        let removed = self.conn.execute(
            "DELETE FROM media_assets WHERE blake3_hash = ?1",
            params![blake3_hash],
        )?;
        Ok(removed > 0)
    }

    pub fn move_media_to_bin(
        &self,
        blake3_hash: &str,
        target_bin_id: &str,
    ) -> Result<bool, LibraryError> {
        let bin_id = if target_bin_id.trim().is_empty() || target_bin_id == "all" {
            DEFAULT_MEDIA_BIN_ID
        } else {
            target_bin_id
        };
        let updated = self.conn.execute(
            "UPDATE media_assets SET bin_id = ?1 WHERE blake3_hash = ?2",
            params![bin_id, blake3_hash],
        )?;
        Ok(updated > 0)
    }

    pub fn list_media_assets(&self) -> Result<Vec<MediaAssetRecord>, LibraryError> {
        self.list_media_assets_filtered(None, None)
    }

    pub fn list_media_assets_filtered(
        &self,
        kind: Option<&str>,
        bin_id: Option<&str>,
    ) -> Result<Vec<MediaAssetRecord>, LibraryError> {
        let mut stmt = self.conn.prepare(
            "SELECT blake3_hash, bin_id, file_name, local_path, kind, width, height, duration_ms, has_alpha, size_bytes FROM media_assets ORDER BY file_name COLLATE NOCASE ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(MediaAssetRecord {
                blake3_hash: row.get(0)?,
                bin_id: row.get(1)?,
                file_name: row.get(2)?,
                local_path: row.get(3)?,
                kind: row.get(4)?,
                width: row.get(5)?,
                height: row.get(6)?,
                duration_ms: row.get(7)?,
                has_alpha: row.get::<_, i32>(8)? != 0,
                size_bytes: row.get::<_, i64>(9)? as u64,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            let item = r?;
            if let Some(k) = kind
                && !k.is_empty()
                && k != "all"
                && !item.kind.eq_ignore_ascii_case(k)
            {
                continue;
            }
            if let Some(b) = bin_id
                && !b.is_empty()
                && b != "all"
                && item.bin_id != b
            {
                continue;
            }
            out.push(item);
        }
        Ok(out)
    }

    pub fn insert_bible_verses(&mut self, verses: &[BibleVerseRecord]) -> Result<usize, LibraryError> {
        let tx = self.conn.transaction()?;
        {
            let mut insert_main = tx.prepare(
                r#"
                INSERT OR REPLACE INTO bible_verses (translation, book, chapter, verse, text, attribution, cached_at_epoch)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
            )?;
            let mut insert_fts = tx.prepare(
                "INSERT INTO bible_verses_fts (translation, book, chapter, verse, text) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for v in verses {
                insert_main.execute(params![
                    v.translation,
                    v.book,
                    v.chapter,
                    v.verse,
                    v.text,
                    v.attribution,
                    v.cached_at_epoch,
                ])?;
                insert_fts.execute(params![
                    v.translation,
                    v.book,
                    v.chapter,
                    v.verse,
                    v.text,
                ])?;
            }
        }
        tx.commit()?;
        Ok(verses.len())
    }

    pub fn list_bible_translations(&self) -> Result<Vec<String>, LibraryError> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT translation FROM bible_verses ORDER BY translation ASC")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Looks up Bible passages by reference (e.g. `"John 3:16-17"`, `"Psalm 23"`, `"Ps 100:1-2"`)
    /// or falls back to offline SQLite FTS5 keyword search when no reference matches.
    pub fn lookup_scripture(
        &self,
        translation: Option<&str>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<BibleVerseRecord>, LibraryError> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            let mut stmt = if translation.is_some() {
                self.conn.prepare(
                    "SELECT translation, book, chapter, verse, text, attribution, cached_at_epoch FROM bible_verses WHERE translation = ?1 ORDER BY book, chapter, verse LIMIT ?2",
                )?
            } else {
                self.conn.prepare(
                    "SELECT translation, book, chapter, verse, text, attribution, cached_at_epoch FROM bible_verses ORDER BY translation, book, chapter, verse LIMIT ?1",
                )?
            };
            let mut out = Vec::new();
            if let Some(tr) = translation {
                for r in stmt.query_map(params![tr, limit as i64], row_to_verse)? {
                    out.push(r?);
                }
            } else {
                for r in stmt.query_map(params![limit as i64], row_to_verse)? {
                    out.push(r?);
                }
            }
            return Ok(out);
        }

        if let Some((book_prefix, chapter, v_start, v_end)) = parse_scripture_reference(trimmed) {
            let like_pat = format!("{book_prefix}%");
            let mut out = Vec::new();
            if let Some(tr) = translation {
                let mut stmt = self.conn.prepare(
                    r#"
                    SELECT translation, book, chapter, verse, text, attribution, cached_at_epoch
                    FROM bible_verses
                    WHERE translation = ?1 AND lower(book) LIKE lower(?2) AND chapter = ?3
                      AND verse >= ?4 AND verse <= ?5
                    ORDER BY verse ASC
                    LIMIT ?6
                    "#,
                )?;
                for r in stmt.query_map(
                    params![tr, like_pat, chapter, v_start, v_end, limit as i64],
                    row_to_verse,
                )? {
                    out.push(r?);
                }
            } else {
                let mut stmt = self.conn.prepare(
                    r#"
                    SELECT translation, book, chapter, verse, text, attribution, cached_at_epoch
                    FROM bible_verses
                    WHERE lower(book) LIKE lower(?1) AND chapter = ?2
                      AND verse >= ?3 AND verse <= ?4
                    ORDER BY translation, verse ASC
                    LIMIT ?5
                    "#,
                )?;
                for r in stmt.query_map(
                    params![like_pat, chapter, v_start, v_end, limit as i64],
                    row_to_verse,
                )? {
                    out.push(r?);
                }
            }
            if !out.is_empty() {
                return Ok(out);
            }
        }

        self.search_bible(translation, trimmed, limit)
    }

    pub fn search_bible(
        &self,
        translation: Option<&str>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<BibleVerseRecord>, LibraryError> {
        let fts = sanitize_fts_query(query);
        if fts.is_empty() {
            return Ok(Vec::new());
        }
        let sql = if translation.is_some() {
            r#"
            SELECT b.translation, b.book, b.chapter, b.verse, b.text, b.attribution, b.cached_at_epoch
            FROM bible_verses_fts f
            JOIN bible_verses b
              ON b.translation = f.translation AND b.book = f.book AND b.chapter = f.chapter AND b.verse = f.verse
            WHERE bible_verses_fts MATCH ?1 AND b.translation = ?2
            LIMIT ?3
            "#
        } else {
            r#"
            SELECT b.translation, b.book, b.chapter, b.verse, b.text, b.attribution, b.cached_at_epoch
            FROM bible_verses_fts f
            JOIN bible_verses b
              ON b.translation = f.translation AND b.book = f.book AND b.chapter = f.chapter AND b.verse = f.verse
            WHERE bible_verses_fts MATCH ?1
            LIMIT ?2
            "#
        };
        let mut stmt = self.conn.prepare(sql)?;
        let mut out = Vec::new();
        if let Some(tr) = translation {
            let rows = stmt.query_map(params![fts, tr, limit as i64], row_to_verse)?;
            for r in rows {
                out.push(r?);
            }
        } else {
            let rows = stmt.query_map(params![fts, limit as i64], row_to_verse)?;
            for r in rows {
                out.push(r?);
            }
        }
        Ok(out)
    }

    /// Purges API.Bible cached verses older than `max_age_secs` (enforces mandatory 30-day rule).
    pub fn purge_expired_cached_verses(&self, now_epoch: i64, max_age_secs: i64) -> Result<usize, LibraryError> {
        let cutoff = now_epoch - max_age_secs;
        let deleted = self.conn.execute(
            "DELETE FROM bible_verses WHERE cached_at_epoch IS NOT NULL AND cached_at_epoch < ?1",
            params![cutoff],
        )?;
        Ok(deleted)
    }

    pub fn record_ccli_usage(
        &self,
        service_profile: &str,
        song_title: &str,
        ccli_number: &str,
        author: &str,
        display_seconds: u32,
    ) -> Result<(), LibraryError> {
        self.conn.execute(
            r#"
            INSERT INTO ccli_usage_log (projected_at, service_profile, song_title, ccli_number, author, display_seconds)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                Utc::now().to_rfc3339(),
                service_profile,
                song_title,
                ccli_number,
                author,
                display_seconds,
            ],
        )?;
        Ok(())
    }

    pub fn list_ccli_usage(&self) -> Result<Vec<CcliUsageEntry>, LibraryError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, projected_at, service_profile, song_title, ccli_number, author, display_seconds FROM ccli_usage_log ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CcliUsageEntry {
                id: row.get(0)?,
                projected_at: row.get(1)?,
                service_profile: row.get(2)?,
                song_title: row.get(3)?,
                ccli_number: row.get(4)?,
                author: row.get(5)?,
                display_seconds: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn save_crdt_snapshot(&self, doc_id: &str, blob: &[u8]) -> Result<(), LibraryError> {
        self.conn.execute(
            r#"
            INSERT INTO crdt_snapshots (doc_id, snapshot_blob, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(doc_id) DO UPDATE SET
                snapshot_blob = excluded.snapshot_blob,
                updated_at = excluded.updated_at
            "#,
            params![doc_id, blob, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn load_crdt_snapshot(&self, doc_id: &str) -> Result<Option<Vec<u8>>, LibraryError> {
        self.conn
            .query_row(
                "SELECT snapshot_blob FROM crdt_snapshots WHERE doc_id = ?1",
                params![doc_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(LibraryError::from)
    }

    pub fn upsert_look(&self, look: &LookConfig) -> Result<(), LibraryError> {
        let json = serde_json::to_string(look)?;
        self.conn.execute(
            r#"
            INSERT INTO looks (id, name, config_json, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                config_json = excluded.config_json,
                updated_at = excluded.updated_at
            "#,
            params![look.id.0.to_string(), look.name, json, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn list_looks(&self) -> Result<Vec<LookConfig>, LibraryError> {
        let mut stmt = self
            .conn
            .prepare("SELECT config_json FROM looks ORDER BY name COLLATE NOCASE ASC")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            let json = r?;
            if let Ok(look) = serde_json::from_str::<LookConfig>(&json) {
                out.push(look);
            }
        }
        Ok(out)
    }

    pub fn delete_look(&self, id: LookId) -> Result<bool, LibraryError> {
        if id == LOOK_AUDIENCE_FULL || id == LOOK_STAGE_CONFIDENCE {
            return Ok(false);
        }
        let removed = self
            .conn
            .execute("DELETE FROM looks WHERE id = ?1", params![id.0.to_string()])?;
        Ok(removed > 0)
    }

    pub fn upsert_template(&self, tpl: &TemplateConfig) -> Result<(), LibraryError> {
        let json = serde_json::to_string(tpl)?;
        self.conn.execute(
            r#"
            INSERT INTO templates (id, name, category, config_json, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                category = excluded.category,
                config_json = excluded.config_json,
                updated_at = excluded.updated_at
            "#,
            params![
                tpl.id.0.to_string(),
                tpl.name,
                tpl.category,
                json,
                Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn list_templates(&self) -> Result<Vec<TemplateConfig>, LibraryError> {
        let mut stmt = self
            .conn
            .prepare("SELECT config_json FROM templates ORDER BY category COLLATE NOCASE ASC, name COLLATE NOCASE ASC")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            let json = r?;
            if let Ok(tpl) = serde_json::from_str::<TemplateConfig>(&json) {
                out.push(tpl);
            }
        }
        Ok(out)
    }

    pub fn delete_template(&self, id: TemplateId) -> Result<bool, LibraryError> {
        let removed = self.conn.execute(
            "DELETE FROM templates WHERE id = ?1",
            params![id.0.to_string()],
        )?;
        Ok(removed > 0)
    }

    pub fn upsert_look_preset(&self, preset: &LookPresetRecord) -> Result<(), LibraryError> {
        let json = serde_json::to_string(&preset.assignments)?;
        self.conn.execute(
            r#"
            INSERT INTO look_presets (id, name, assignments_json, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                assignments_json = excluded.assignments_json,
                updated_at = excluded.updated_at
            "#,
            params![
                preset.id,
                preset.name,
                json,
                preset.updated_at.to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn list_look_presets(&self) -> Result<Vec<LookPresetRecord>, LibraryError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, assignments_json, updated_at FROM look_presets ORDER BY name COLLATE NOCASE ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let assignments_json: String = row.get(2)?;
            let updated_str: String = row.get(3)?;
            Ok(LookPresetRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                assignments: serde_json::from_str(&assignments_json).unwrap_or_default(),
                updated_at: DateTime::parse_from_rfc3339(&updated_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn delete_look_preset(&self, id: &str) -> Result<bool, LibraryError> {
        let removed = self
            .conn
            .execute("DELETE FROM look_presets WHERE id = ?1", params![id])?;
        Ok(removed > 0)
    }

    pub fn upsert_overlay(&self, ov: &OverlayItem) -> Result<(), LibraryError> {
        let json = serde_json::to_string(ov)?;
        self.conn.execute(
            "INSERT INTO overlays (id, name, config_json, updated_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(id) DO UPDATE SET name = excluded.name, config_json = excluded.config_json, updated_at = excluded.updated_at",
            params![ov.id, ov.name, json, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn list_overlays(&self) -> Result<Vec<OverlayItem>, LibraryError> {
        let mut stmt = self
            .conn
            .prepare("SELECT config_json FROM overlays ORDER BY name COLLATE NOCASE ASC")?;
        let mut out = Vec::new();
        for r in stmt.query_map([], |row| row.get::<_, String>(0))? {
            if let Ok(item) = serde_json::from_str::<OverlayItem>(&r?) {
                out.push(item);
            }
        }
        Ok(out)
    }

    pub fn delete_overlay(&self, id: &str) -> Result<bool, LibraryError> {
        let removed = self
            .conn
            .execute("DELETE FROM overlays WHERE id = ?1", params![id])?;
        Ok(removed > 0)
    }

    pub fn upsert_message_template(&self, tpl: &MessageTemplate) -> Result<(), LibraryError> {
        let json = serde_json::to_string(tpl)?;
        self.conn.execute(
            "INSERT INTO message_templates (id, name, config_json, updated_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(id) DO UPDATE SET name = excluded.name, config_json = excluded.config_json, updated_at = excluded.updated_at",
            params![tpl.id, tpl.name, json, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn list_message_templates(&self) -> Result<Vec<MessageTemplate>, LibraryError> {
        let mut stmt = self
            .conn
            .prepare("SELECT config_json FROM message_templates ORDER BY name COLLATE NOCASE ASC")?;
        let mut out = Vec::new();
        for r in stmt.query_map([], |row| row.get::<_, String>(0))? {
            if let Ok(item) = serde_json::from_str::<MessageTemplate>(&r?) {
                out.push(item);
            }
        }
        Ok(out)
    }

    pub fn delete_message_template(&self, id: &str) -> Result<bool, LibraryError> {
        let removed = self
            .conn
            .execute("DELETE FROM message_templates WHERE id = ?1", params![id])?;
        Ok(removed > 0)
    }

    pub fn upsert_macro(&self, rec: &MacroRecord) -> Result<(), LibraryError> {
        let json = serde_json::to_string(&rec.steps)?;
        self.conn.execute(
            "INSERT INTO macros (id, name, steps_json, updated_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(id) DO UPDATE SET name = excluded.name, steps_json = excluded.steps_json, updated_at = excluded.updated_at",
            params![rec.id, rec.name, json, rec.updated_at.to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn list_macros(&self) -> Result<Vec<MacroRecord>, LibraryError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, steps_json, updated_at FROM macros ORDER BY name COLLATE NOCASE ASC")?;
        let rows = stmt.query_map([], |row| {
            let steps_json: String = row.get(2)?;
            let updated_str: String = row.get(3)?;
            Ok(MacroRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                steps: serde_json::from_str(&steps_json).unwrap_or_default(),
                updated_at: DateTime::parse_from_rfc3339(&updated_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn delete_macro(&self, id: &str) -> Result<bool, LibraryError> {
        let removed = self
            .conn
            .execute("DELETE FROM macros WHERE id = ?1", params![id])?;
        Ok(removed > 0)
    }

    pub fn upsert_scheduled_event(&self, ev: &ScheduledEventRecord) -> Result<(), LibraryError> {
        self.conn.execute(
            "INSERT INTO scheduled_events (id, name, trigger_hhmm, macro_id, enabled, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(id) DO UPDATE SET name = excluded.name, trigger_hhmm = excluded.trigger_hhmm, macro_id = excluded.macro_id, enabled = excluded.enabled, updated_at = excluded.updated_at",
            params![
                ev.id,
                ev.name,
                ev.trigger_hhmm,
                ev.macro_id,
                i32::from(ev.enabled),
                ev.updated_at.to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn list_scheduled_events(&self) -> Result<Vec<ScheduledEventRecord>, LibraryError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, trigger_hhmm, macro_id, enabled, updated_at FROM scheduled_events ORDER BY trigger_hhmm ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let updated_str: String = row.get(5)?;
            Ok(ScheduledEventRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                trigger_hhmm: row.get(2)?,
                macro_id: row.get(3)?,
                enabled: row.get::<_, i32>(4)? != 0,
                updated_at: DateTime::parse_from_rfc3339(&updated_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn delete_scheduled_event(&self, id: &str) -> Result<bool, LibraryError> {
        let removed = self
            .conn
            .execute("DELETE FROM scheduled_events WHERE id = ?1", params![id])?;
        Ok(removed > 0)
    }

    /// Seeds default presentation libraries, media bins, starter media backgrounds,
    /// looks, templates, demo presentations, and a Sunday Morning Service playlist if empty.
    pub fn seed_demo_if_empty(&self) -> Result<(), LibraryError> {
        let now = Utc::now();
        if self.list_templates()?.is_empty() {
            for tpl in default_templates() {
                self.upsert_template(&tpl)?;
            }
        }
        if self.list_looks()?.is_empty() {
            for look in default_looks() {
                self.upsert_look(&look)?;
            }
        }
        if self.list_look_presets()?.is_empty() {
            let mut worship_map = BTreeMap::new();
            worship_map.insert(
                core_scene::OUTPUT_AUDIENCE_MAIN.0.to_string(),
                LOOK_AUDIENCE_FULL.0.to_string(),
            );
            worship_map.insert(
                core_scene::OUTPUT_STAGE_MAIN.0.to_string(),
                LOOK_STAGE_CONFIDENCE.0.to_string(),
            );
            self.upsert_look_preset(&LookPresetRecord {
                id: "preset-worship-full".to_string(),
                name: "Worship Full Sanctuary".to_string(),
                assignments: worship_map,
                updated_at: now,
            })?;

            let mut broadcast_map = BTreeMap::new();
            broadcast_map.insert(
                core_scene::OUTPUT_AUDIENCE_MAIN.0.to_string(),
                core_scene::LOOK_BROADCAST_LOWER_THIRD.0.to_string(),
            );
            broadcast_map.insert(
                core_scene::OUTPUT_STAGE_MAIN.0.to_string(),
                LOOK_STAGE_CONFIDENCE.0.to_string(),
            );
            self.upsert_look_preset(&LookPresetRecord {
                id: "preset-broadcast-lower-third".to_string(),
                name: "Livestream Lower-Third".to_string(),
                assignments: broadcast_map,
                updated_at: now,
            })?;
        }
        if self.list_libraries()?.is_empty() {
            for (id, name) in [
                (DEFAULT_LIBRARY_ID, "Default Library"),
                ("worship-songs", "Worship Songs"),
                ("sermon-series", "Sermon & Announcements"),
            ] {
                self.upsert_library(&PresentationLibraryRecord {
                    id: id.to_string(),
                    name: name.to_string(),
                    updated_at: now,
                })?;
            }
        }

        if self.list_media_bins()?.is_empty() {
            for (id, name, category) in [
                (DEFAULT_MEDIA_BIN_ID, "Motion Backgrounds", "background"),
                ("images", "Still Images & Slides", "image"),
                ("videos", "Videos & Countdowns", "video"),
                ("audio", "Audio & Walk-In Music", "audio"),
            ] {
                self.upsert_media_bin(&MediaBinRecord {
                    id: id.to_string(),
                    name: name.to_string(),
                    category: category.to_string(),
                    updated_at: now,
                })?;
            }
        }

        if self.list_media_assets()?.is_empty() {
            for (hash, bin_id, name, path, kind, has_alpha) in [
                (
                    "builtin-bg-aurora-wave",
                    "backgrounds",
                    "Aurora Wave (GPU Motion)",
                    "builtin://motion/aurora-wave",
                    "background",
                    false,
                ),
                (
                    "builtin-bg-sanctuary-glow",
                    "backgrounds",
                    "Sanctuary Glow (GPU Motion)",
                    "builtin://motion/sanctuary-glow",
                    "background",
                    false,
                ),
                (
                    "builtin-bg-broadcast-grid",
                    "images",
                    "Broadcast Safe-Area Test Card",
                    "builtin://motion/broadcast-grid",
                    "image",
                    false,
                ),
                (
                    "builtin-bg-lower-third-alpha",
                    "videos",
                    "Lower-Third Alpha Key Band",
                    "builtin://motion/lower-third-alpha",
                    "video",
                    true,
                ),
            ] {
                self.upsert_media_asset(&MediaAssetRecord {
                    blake3_hash: hash.to_string(),
                    bin_id: bin_id.to_string(),
                    file_name: name.to_string(),
                    local_path: path.to_string(),
                    kind: kind.to_string(),
                    width: Some(1920),
                    height: Some(1080),
                    duration_ms: None,
                    has_alpha,
                    size_bytes: 0,
                })?;
            }
        }

        if self.list_overlays()?.is_empty() {
            for ov in default_overlays() {
                self.upsert_overlay(&ov)?;
            }
        }
        if self.list_message_templates()?.is_empty() {
            for mt in default_message_templates() {
                self.upsert_message_template(&mt)?;
            }
        }
        if self.list_macros()?.is_empty() {
            self.upsert_macro(&MacroRecord {
                id: "macro-worship-start".to_string(),
                name: "Start Worship Service".to_string(),
                steps: vec![
                    "blackout:off".to_string(),
                    "overlay:show:overlay-church-bug".to_string(),
                    "mask:None".to_string(),
                ],
                updated_at: now,
            })?;
            self.upsert_macro(&MacroRecord {
                id: "macro-clear-reset".to_string(),
                name: "Clear All & Reset".to_string(),
                steps: vec!["clear:all".to_string(), "blackout:off".to_string()],
                updated_at: now,
            })?;
        }
        if self.list_scheduled_events()?.is_empty() {
            self.upsert_scheduled_event(&ScheduledEventRecord {
                id: "sched-sunday-worship".to_string(),
                name: "Sunday 10:00 AM Worship Start".to_string(),
                trigger_hhmm: "10:00".to_string(),
                macro_id: "macro-worship-start".to_string(),
                enabled: false,
                updated_at: now,
            })?;
        }

        if self.list_bible_translations()?.is_empty() {
            let starter = starter_public_domain_scripture();
            for v in &starter {
                let _ = self.conn.execute(
                    "INSERT OR REPLACE INTO bible_verses (translation, book, chapter, verse, text, attribution, cached_at_epoch) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![v.translation, v.book, v.chapter, v.verse, v.text, v.attribution, v.cached_at_epoch],
                );
                let _ = self.conn.execute(
                    "INSERT INTO bible_verses_fts (translation, book, chapter, verse, text) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![v.translation, v.book, v.chapter, v.verse, v.text],
                );
            }
        }

        if !self.list_songs()?.is_empty() {
            return Ok(());
        }

        let deck = core_scene::demo_deck();
        let welcome = SongRecord {
            id: Uuid::from_u128(9000).to_string(),
            library_id: "sermon-series".to_string(),
            title: "Sunday Welcome & Announcements".to_string(),
            author: "Production Team".to_string(),
            copyright: String::new(),
            ccli_number: None,
            key_signature: None,
            bpm: None,
            lyrics_plain: deck[0].plain_text(),
            slides: vec![deck[0].clone(), deck[3].clone(), deck[4].clone()],
            arrangements: Vec::new(),
            updated_at: now,
        };
        let song = SongRecord {
            id: Uuid::from_u128(9001).to_string(),
            library_id: "worship-songs".to_string(),
            title: "Amazing Grace".to_string(),
            author: "John Newton".to_string(),
            copyright: "Public Domain".to_string(),
            ccli_number: Some("22025".to_string()),
            key_signature: Some("G".to_string()),
            bpm: Some(72.0),
            lyrics_plain: deck[1].plain_text(),
            slides: vec![deck[1].clone()],
            arrangements: vec![Arrangement {
                id: "arr-default".to_string(),
                name: "Default".to_string(),
                group_order: vec!["Verse 1".to_string()],
            }],
            updated_at: now,
        };
        let scripture = SongRecord {
            id: Uuid::from_u128(9002).to_string(),
            library_id: DEFAULT_LIBRARY_ID.to_string(),
            title: "Psalm 100:1-2 (KJV)".to_string(),
            author: "Scripture Reading".to_string(),
            copyright: "Public Domain".to_string(),
            ccli_number: None,
            key_signature: None,
            bpm: None,
            lyrics_plain: deck[2].plain_text(),
            slides: vec![deck[2].clone()],
            arrangements: Vec::new(),
            updated_at: now,
        };
        self.upsert_song(&welcome)?;
        self.upsert_song(&song)?;
        self.upsert_song(&scripture)?;

        let playlist = PlaylistRecord {
            id: Uuid::from_u128(9101).to_string(),
            name: "Sunday Morning Service".to_string(),
            item_song_ids: vec![welcome.id, song.id, scripture.id],
            updated_at: now,
        };
        self.upsert_playlist(&playlist)?;
        Ok(())
    }
}

/// Infers the media asset category (`background`, `image`, `video`, `audio`) from file extension.
pub fn infer_media_kind(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" | "svg" | "tif" | "tiff") => "image",
        Some("mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "opus") => "audio",
        Some("mp4" | "mov" | "webm" | "mkv" | "m4v" | "avi") => "video",
        _ => "background",
    }
}

fn row_to_song(row: &rusqlite::Row<'_>) -> rusqlite::Result<SongRecord> {
    let slides_json: String = row.get(9)?;
    let arrangements_json: String = row.get(10)?;
    let updated_str: String = row.get(11)?;
    let slides: Vec<Slide> = serde_json::from_str(&slides_json).unwrap_or_default();
    let arrangements: Vec<Arrangement> =
        serde_json::from_str(&arrangements_json).unwrap_or_default();
    Ok(SongRecord {
        id: row.get(0)?,
        library_id: row.get(1)?,
        title: row.get(2)?,
        author: row.get(3)?,
        copyright: row.get(4)?,
        ccli_number: row.get(5)?,
        key_signature: row.get(6)?,
        bpm: row.get(7)?,
        lyrics_plain: row.get(8)?,
        slides,
        arrangements,
        updated_at: DateTime::parse_from_rfc3339(&updated_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

fn row_to_verse(row: &rusqlite::Row<'_>) -> rusqlite::Result<BibleVerseRecord> {
    Ok(BibleVerseRecord {
        translation: row.get(0)?,
        book: row.get(1)?,
        chapter: row.get(2)?,
        verse: row.get(3)?,
        text: row.get(4)?,
        attribution: row.get(5)?,
        cached_at_epoch: row.get(6)?,
    })
}

fn sanitize_fts_query(raw: &str) -> String {
    raw.split_whitespace()
        .map(|tok| {
            let clean: String = tok
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if clean.is_empty() {
                String::new()
            } else {
                format!("{clean}*")
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Helper to build a quick single-slide song record from imported text.
pub fn slide_from_text(title: &str, group: &str, body: &str, ccli: Option<String>) -> Slide {
    let mut base = core_scene::demo_deck()[0].clone();
    base.id = SlideId::new();
    base.label = title.to_string();
    base.group = group.to_string();
    base.headline = body.lines().next().unwrap_or(title).to_string();
    base.body = body.to_string();
    base.ccli_number = ccli;
    if let Some(el) = base.elements.first_mut()
        && let core_scene::SlideElementKind::TextBox { ref mut text, .. } = el.kind
    {
        *text = body.to_string();
    }
    base.elements.truncate(1);
    base
}

/// Parses Quick Lyrics with `[Verse 1]` / `[Chorus]` / `[Bridge]` / `[Tag]` group headers,
/// auto-splits stanzas by `max_lines_per_slide`, supports dual-language lines (`Primary // Secondary`),
/// and builds both the master slides and a default [`Arrangement`].
pub fn parse_quick_lyrics(
    title: &str,
    raw_text: &str,
    max_lines_per_slide: usize,
    ccli: Option<String>,
) -> (Vec<Slide>, Vec<Arrangement>) {
    let cap = max_lines_per_slide.clamp(1, 12);
    let mut sections: Vec<(String, Vec<String>)> = Vec::new();
    let mut current_group = "Verse 1".to_string();
    let mut current_lines: Vec<String> = Vec::new();
    let mut verse_counter = 1usize;

    for raw_line in raw_text.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            if !current_lines.is_empty() {
                sections.push((current_group.clone(), std::mem::take(&mut current_lines)));
                verse_counter += 1;
                current_group = format!("Verse {verse_counter}");
            }
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() > 2 {
            if !current_lines.is_empty() {
                sections.push((current_group.clone(), std::mem::take(&mut current_lines)));
            }
            current_group = trimmed[1..trimmed.len() - 1].trim().to_string();
            continue;
        }
        current_lines.push(trimmed.to_string());
    }
    if !current_lines.is_empty() {
        sections.push((current_group, current_lines));
    }

    let mut slides = Vec::new();
    let mut group_order = Vec::new();
    let mut seen_groups: Vec<String> = Vec::new();

    for (group, lines) in sections {
        group_order.push(group.clone());
        if seen_groups.iter().any(|g| g.eq_ignore_ascii_case(&group)) {
            continue;
        }
        seen_groups.push(group.clone());

        let chunks: Vec<&[String]> = lines.chunks(cap).collect();
        let total = chunks.len();
        for (idx, chunk) in chunks.into_iter().enumerate() {
            let mut primary_lines = Vec::new();
            let mut secondary_lines = Vec::new();
            for line in chunk {
                if let Some((a, b)) = line.split_once("//") {
                    primary_lines.push(a.trim().to_string());
                    secondary_lines.push(b.trim().to_string());
                } else {
                    primary_lines.push(line.clone());
                }
            }
            let primary_text = primary_lines.join("\n");
            let label = if total > 1 {
                format!("{title} — {group} ({}/{})", idx + 1, total)
            } else {
                format!("{title} — {group}")
            };
            let mut slide = slide_from_text(&label, &group, &primary_text, ccli.clone());
            if !secondary_lines.is_empty() {
                let sec_text = secondary_lines.join("\n");
                slide.body = sec_text.clone();
                slide.elements.push(core_scene::SlideElement {
                    id: core_scene::ElementId::new(),
                    name: "Secondary Translation".to_string(),
                    bounds: core_scene::RectF::new(200.0, 760.0, 1520.0, 180.0),
                    opacity: 0.92,
                    z_index: 11,
                    kind: core_scene::SlideElementKind::TextBox {
                        text: sec_text,
                        style: core_scene::TextStyle {
                            font_size: 50.0,
                            bold: false,
                            italic: true,
                            color: core_scene::ColorRgba::rgba(0.95, 0.86, 0.56, 0.96),
                            stroke_width: 2.5,
                            ..core_scene::TextStyle::default()
                        },
                    },
                });
            }
            slides.push(slide);
        }
    }

    if slides.is_empty() {
        slides.push(slide_from_text(title, "Verse 1", title, ccli));
        group_order.push("Verse 1".to_string());
    }

    let arrangements = vec![Arrangement {
        id: "arr-default".to_string(),
        name: "Default".to_string(),
        group_order,
    }];
    (slides, arrangements)
}

/// Parses a scripture reference string such as `"John 3:16-17"`, `"Psalm 100:1-2"`, or `"Ps 23"`.
pub fn parse_scripture_reference(raw: &str) -> Option<(String, u32, u32, u32)> {
    let trimmed = raw.trim();
    let last_space = trimmed.rfind(' ')?;
    let book_raw = trimmed[..last_space].trim();
    let num_part = trimmed[last_space + 1..].trim();
    if book_raw.is_empty() || num_part.is_empty() {
        return None;
    }
    let normalized_book = match book_raw.to_ascii_lowercase().as_str() {
        "ps" | "psa" | "psalms" => "Psalm".to_string(),
        "gen" => "Genesis".to_string(),
        "mat" | "matt" => "Matthew".to_string(),
        "rom" => "Romans".to_string(),
        "num" => "Numbers".to_string(),
        "prov" | "pro" => "Proverbs".to_string(),
        "jn" | "jhn" => "John".to_string(),
        _ => book_raw.to_string(),
    };

    if let Some((ch_str, vs_str)) = num_part.split_once(':') {
        let chapter: u32 = ch_str.trim().parse().ok()?;
        if let Some((s, e)) = vs_str.split_once('-') {
            let start: u32 = s.trim().parse().ok()?;
            let end: u32 = e.trim().parse().ok()?;
            Some((normalized_book, chapter, start.min(end), start.max(end)))
        } else {
            let v: u32 = vs_str.trim().parse().ok()?;
            Some((normalized_book, chapter, v, v))
        }
    } else {
        let chapter: u32 = num_part.parse().ok()?;
        Some((normalized_book, chapter, 1, 200))
    }
}

/// Builds presentation slides from primary Bible verses (with optional parallel secondary translation).
pub fn build_scripture_slides(
    primary: &[BibleVerseRecord],
    secondary: &[BibleVerseRecord],
    verses_per_slide: usize,
    show_verse_numbers: bool,
) -> Vec<Slide> {
    let per_slide = verses_per_slide.clamp(1, 6);
    let mut out = Vec::new();
    for chunk in primary.chunks(per_slide) {
        let Some(first) = chunk.first() else {
            continue;
        };
        let Some(last) = chunk.last() else {
            continue;
        };
        let range_ref = if first.verse == last.verse {
            format!("{} {}:{}", first.book, first.chapter, first.verse)
        } else {
            format!("{} {}:{}-{}", first.book, first.chapter, first.verse, last.verse)
        };
        let body_text = chunk
            .iter()
            .map(|v| {
                if show_verse_numbers {
                    format!("{} {}", v.verse, v.text)
                } else {
                    v.text.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        let sec_matches: Vec<&BibleVerseRecord> = chunk
            .iter()
            .filter_map(|pv| {
                secondary.iter().find(|sv| {
                    sv.book.eq_ignore_ascii_case(&pv.book)
                        && sv.chapter == pv.chapter
                        && sv.verse == pv.verse
                })
            })
            .collect();

        let ref_caption = if let Some(sec_first) = sec_matches.first() {
            format!(
                "{range_ref} ({} / {})",
                first.translation, sec_first.translation
            )
        } else {
            format!("{range_ref} ({})", first.translation)
        };

        let mut slide = slide_from_text(&ref_caption, "Scripture", &body_text, None);
        slide.headline = ref_caption.clone();
        slide.body = body_text.clone();
        slide.primary_color = core_scene::ColorRgba::rgb(0.07, 0.09, 0.19);
        slide.secondary_color = core_scene::ColorRgba::rgb(0.22, 0.18, 0.44);

        if !sec_matches.is_empty() {
            let sec_text = sec_matches
                .iter()
                .map(|v| {
                    if show_verse_numbers {
                        format!("{} {}", v.verse, v.text)
                    } else {
                        v.text.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            if let Some(el) = slide.elements.first_mut() {
                el.bounds = core_scene::RectF::new(160.0, 140.0, 1600.0, 350.0);
            }
            slide.elements.push(core_scene::SlideElement {
                id: core_scene::ElementId::new(),
                name: "Parallel Translation".to_string(),
                bounds: core_scene::RectF::new(160.0, 520.0, 1600.0, 280.0),
                opacity: 0.94,
                z_index: 11,
                kind: core_scene::SlideElementKind::TextBox {
                    text: sec_text,
                    style: core_scene::TextStyle {
                        font_size: 52.0,
                        bold: false,
                        italic: true,
                        color: core_scene::ColorRgba::rgba(0.95, 0.87, 0.62, 0.96),
                        stroke_width: 2.0,
                        ..core_scene::TextStyle::default()
                    },
                },
            });
        }

        slide.elements.push(core_scene::SlideElement {
            id: core_scene::ElementId::new(),
            name: "Scripture Reference".to_string(),
            bounds: core_scene::RectF::new(180.0, 830.0, 1560.0, 110.0),
            opacity: 0.95,
            z_index: 12,
            kind: core_scene::SlideElementKind::TextBox {
                text: ref_caption,
                style: core_scene::TextStyle {
                    font_size: 42.0,
                    bold: true,
                    color: core_scene::ColorRgba::rgba(0.96, 0.80, 0.36, 0.98),
                    stroke_width: 2.0,
                    h_align: core_scene::TextAlign::Right,
                    ..core_scene::TextStyle::default()
                },
            },
        });
        out.push(slide);
    }
    out
}

fn starter_public_domain_scripture() -> Vec<BibleVerseRecord> {
    let entries: &[(&str, &str, u32, u32, &str)] = &[
        ("KJV", "Genesis", 1, 1, "In the beginning God created the heaven and the earth."),
        ("KJV", "Genesis", 1, 2, "And the earth was without form, and void; and darkness was upon the face of the deep."),
        ("KJV", "Genesis", 1, 3, "And God said, Let there be light: and there was light."),
        ("KJV", "Numbers", 6, 24, "The LORD bless thee, and keep thee:"),
        ("KJV", "Numbers", 6, 25, "The LORD make his face shine upon thee, and be gracious unto thee:"),
        ("KJV", "Numbers", 6, 26, "The LORD lift up his countenance upon thee, and give thee peace."),
        ("KJV", "Psalm", 23, 1, "The LORD is my shepherd; I shall not want."),
        ("KJV", "Psalm", 23, 2, "He maketh me to lie down in green pastures: he leadeth me beside the still waters."),
        ("KJV", "Psalm", 23, 3, "He restoreth my soul: he leadeth me in the paths of righteousness for his name's sake."),
        ("KJV", "Psalm", 100, 1, "Make a joyful noise unto the LORD, all ye lands."),
        ("KJV", "Psalm", 100, 2, "Serve the LORD with gladness: come before his presence with singing."),
        ("KJV", "Proverbs", 3, 5, "Trust in the LORD with all thine heart; and lean not unto thine own understanding."),
        ("KJV", "Proverbs", 3, 6, "In all thy ways acknowledge him, and he shall direct thy paths."),
        ("KJV", "Matthew", 28, 19, "Go ye therefore, and teach all nations, baptizing them in the name of the Father, and of the Son, and of the Holy Ghost:"),
        ("KJV", "Matthew", 28, 20, "Teaching them to observe all things whatsoever I have commanded you: and, lo, I am with you alway, even unto the end of the world."),
        ("KJV", "John", 3, 16, "For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life."),
        ("KJV", "John", 3, 17, "For God sent not his Son into the world to condemn the world; but that the world through him might be saved."),
        ("KJV", "Romans", 8, 28, "And we know that all things work together for good to them that love God, to them who are the called according to his purpose."),
        ("WEB", "Genesis", 1, 1, "In the beginning, God created the heavens and the earth."),
        ("WEB", "Psalm", 23, 1, "Yahweh is my shepherd: I shall lack nothing."),
        ("WEB", "Psalm", 23, 2, "He makes me lie down in green pastures. He leads me beside still waters."),
        ("WEB", "Psalm", 100, 1, "Shout for joy to Yahweh, all you lands!"),
        ("WEB", "Psalm", 100, 2, "Serve Yahweh with gladness. Come before his presence with singing."),
        ("WEB", "John", 3, 16, "For God so loved the world, that he gave his one and only Son, that whoever believes in him should not perish, but have eternal life."),
        ("WEB", "John", 3, 17, "For God didn't send his Son into the world to judge the world, but that the world should be saved through him."),
        ("WEB", "Romans", 8, 28, "We know that all things work together for good for those who love God, for those who are called according to his purpose."),
    ];
    entries
        .iter()
        .map(|&(tr, book, ch, v, txt)| BibleVerseRecord {
            translation: tr.to_string(),
            book: book.to_string(),
            chapter: ch,
            verse: v,
            text: txt.to_string(),
            attribution: "Public Domain".to_string(),
            cached_at_epoch: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts5_song_search_and_ccli_logging_work_offline() {
        let store = LibraryStore::open_in_memory().expect("open memory db");
        store.seed_demo_if_empty().expect("seed demo");

        let hits = store.search_songs("wretch", 10).expect("fts5 search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Amazing Grace");
        assert_eq!(hits[0].ccli_number.as_deref(), Some("22025"));

        store
            .record_ccli_usage("Sunday 10am", "Amazing Grace", "22025", "John Newton", 210)
            .expect("log ccli");
        let logs = store.list_ccli_usage().expect("list ccli");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].display_seconds, 210);
    }

    #[test]
    fn bible_fts_and_30_day_cache_expiry_enforced() {
        let mut store = LibraryStore::open_in_memory().expect("open memory db");
        let now = 1_700_000_000i64;
        let thirty_one_days = 31 * 86_400;

        store
            .insert_bible_verses(&[
                BibleVerseRecord {
                    translation: "KJV".into(),
                    book: "Psalm".into(),
                    chapter: 100,
                    verse: 1,
                    text: "Make a joyful noise unto the LORD, all ye lands.".into(),
                    attribution: "Public Domain".into(),
                    cached_at_epoch: None, // Offline PD translation never expires
                },
                BibleVerseRecord {
                    translation: "API-NIV".into(),
                    book: "Psalm".into(),
                    chapter: 100,
                    verse: 1,
                    text: "Shout for joy to the LORD, all the earth.".into(),
                    attribution: "Copyrighted API.Bible".into(),
                    cached_at_epoch: Some(now - thirty_one_days),
                },
            ])
            .expect("insert verses");

        let purged = store
            .purge_expired_cached_verses(now, 30 * 86_400)
            .expect("purge expired");
        assert_eq!(purged, 1);

        let hits = store.search_bible(Some("KJV"), "joyful", 10).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].book, "Psalm");
    }

    #[test]
    fn registers_media_asset_with_blake3_hash() {
        let store = LibraryStore::open_in_memory().expect("open db");
        let tmp = std::env::temp_dir().join(format!("op-media-test-{}.bin", std::process::id()));
        std::fs::write(&tmp, b"fake-prores-4444-alpha-header").expect("write temp");

        let rec = store
            .register_media_file(&tmp, "video", Some(1920), Some(1080), Some(15_000), true)
            .expect("register");
        assert_eq!(rec.blake3_hash.len(), 64);
        assert!(rec.has_alpha);

        let list = store.list_media_assets().expect("list");
        assert_eq!(list.len(), 1);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn disk_store_playlists_crdt_snapshots_and_edge_cases() {
        let db_path = std::env::temp_dir()
            .join(format!("op-lib-test-{}", std::process::id()))
            .join("library.sqlite3");
        let mut store = LibraryStore::open(&db_path).expect("open disk db");
        store.seed_demo_if_empty().expect("seed 1");
        store.seed_demo_if_empty().expect("seed idempotent");

        // Empty query returns full song list; get_song works for hit and miss
        let all = store.search_songs("   ", 10).expect("empty search");
        assert_eq!(all.len(), 3);
        assert!(store.get_song(&all[0].id).expect("get hit").is_some());
        assert!(store.get_song("nonexistent-id").expect("get miss").is_none());

        // Libraries & playlists CRUD
        let libs = store.list_libraries().expect("list libs");
        assert_eq!(libs.len(), 3);
        assert!(!store.delete_library(DEFAULT_LIBRARY_ID).expect("protect default"));
        store
            .upsert_library(&PresentationLibraryRecord {
                id: "youth".into(),
                name: "Youth Night".into(),
                updated_at: Utc::now(),
            })
            .expect("add youth lib");
        assert_eq!(
            store
                .search_songs_in_library(Some("worship-songs"), "wretch", 10)
                .expect("lib search")
                .len(),
            1
        );
        assert!(store.delete_library("youth").expect("del youth lib"));

        let playlists = store.list_playlists().expect("list playlists");
        assert_eq!(playlists.len(), 1);
        store
            .upsert_playlist(&PlaylistRecord {
                id: "pl-temp".into(),
                name: "Midweek Prayer".into(),
                item_song_ids: vec![],
                updated_at: Utc::now(),
            })
            .expect("add playlist");
        assert!(store.delete_playlist("pl-temp").expect("delete playlist"));

        // Media bins & filtered media CRUD + extension inference
        let bins = store.list_media_bins().expect("list bins");
        assert_eq!(bins.len(), 4);
        assert!(!store.delete_media_bin(DEFAULT_MEDIA_BIN_ID).expect("protect default bin"));
        store
            .upsert_media_bin(&MediaBinRecord {
                id: "christmas".into(),
                name: "Christmas Graphics".into(),
                category: "image".into(),
                updated_at: Utc::now(),
            })
            .expect("add bin");
        assert_eq!(
            store
                .list_media_assets_filtered(Some("background"), Some("backgrounds"))
                .expect("filter bg")
                .len(),
            2
        );
        assert!(store.move_song_to_library(&all[0].id, "worship-songs").expect("move song"));
        assert!(store.move_media_to_bin("builtin-bg-aurora-wave", "images").expect("move media"));
        assert!(store.delete_media_bin("christmas").expect("delete bin"));
        assert!(store.delete_media_asset("builtin-bg-aurora-wave").expect("delete media"));
        assert!(store.delete_song(&all[0].id).expect("delete song"));

        assert_eq!(infer_media_kind(Path::new("slide.PNG")), "image");
        assert_eq!(infer_media_kind(Path::new("loop.mp4")), "video");
        assert_eq!(infer_media_kind(Path::new("walkin.mp3")), "audio");
        assert_eq!(infer_media_kind(Path::new("shader.custom")), "background");

        // Bible search without translation filter & empty query
        store
            .insert_bible_verses(&[BibleVerseRecord {
                translation: "WEB".into(),
                book: "John".into(),
                chapter: 3,
                verse: 16,
                text: "For God so loved the world".into(),
                attribution: "Public Domain".into(),
                cached_at_epoch: None,
            }])
            .expect("insert");
        assert!(!store.search_bible(None, "loved", 5).expect("all trans").is_empty());
        assert!(store.search_bible(None, "   ", 5).expect("blank").is_empty());

        // CRDT snapshot persistence
        assert!(store.load_crdt_snapshot("doc-1").expect("miss").is_none());
        store.save_crdt_snapshot("doc-1", b"loro-binary-blob").expect("save");
        assert_eq!(
            store.load_crdt_snapshot("doc-1").expect("hit").as_deref(),
            Some(b"loro-binary-blob".as_slice())
        );

        let slide = slide_from_text("Test Song", "Chorus", "Line 1\nLine 2", Some("12345".into()));
        assert_eq!(slide.ccli_number.as_deref(), Some("12345"));
        assert!(slide.plain_text().contains("Line 2"));

        // Looks, Templates, and Look Presets persistence CRUD
        assert_eq!(store.list_looks().expect("seeded looks").len(), 3);
        assert_eq!(store.list_templates().expect("seeded templates").len(), 4);
        assert_eq!(store.list_look_presets().expect("seeded presets").len(), 2);

        let mut custom_tpl = default_templates()[0].clone();
        custom_tpl.id = TemplateId::new();
        custom_tpl.name = "Custom Youth Overlay".into();
        custom_tpl.category = "Custom".into();
        store.upsert_template(&custom_tpl).expect("upsert tpl");
        assert_eq!(store.list_templates().expect("list tpls").len(), 5);

        let mut custom_look = default_looks()[0].clone();
        custom_look.id = LookId::new();
        custom_look.name = "Youth Room".into();
        custom_look.template_id = Some(custom_tpl.id);
        store.upsert_look(&custom_look).expect("upsert look");
        assert_eq!(store.list_looks().expect("list looks").len(), 4);

        assert!(!store.delete_look(LOOK_AUDIENCE_FULL).expect("protect audience"));
        assert!(!store.delete_look(LOOK_STAGE_CONFIDENCE).expect("protect stage"));
        assert!(store.delete_look(custom_look.id).expect("delete custom look"));
        assert!(store.delete_template(custom_tpl.id).expect("delete custom tpl"));

        let preset = LookPresetRecord {
            id: "preset-sermon".into(),
            name: "Sermon Mode".into(),
            assignments: BTreeMap::from([(
                core_scene::OUTPUT_AUDIENCE_MAIN.0.to_string(),
                LOOK_AUDIENCE_FULL.0.to_string(),
            )]),
            updated_at: Utc::now(),
        };
        store.upsert_look_preset(&preset).expect("upsert preset");
        assert_eq!(store.list_look_presets().expect("presets").len(), 3);
        assert!(store.delete_look_preset("preset-sermon").expect("delete preset"));

        drop(store);
        let _ = std::fs::remove_dir_all(db_path.parent().expect("parent"));
    }

    #[test]
    fn quick_lyrics_parser_and_scripture_lookup_dual_translation_work_offline() {
        let store = LibraryStore::open_in_memory().expect("open db");
        store.seed_demo_if_empty().expect("seed demo");

        let translations = store.list_bible_translations().expect("list translations");
        assert!(translations.contains(&"KJV".to_string()));
        assert!(translations.contains(&"WEB".to_string()));

        // Reference lookup (single, range, abbreviated book, chapter-only) & FTS5 fallback
        let john = store.lookup_scripture(Some("KJV"), "John 3:16-17", 10).expect("john");
        assert_eq!(john.len(), 2);
        let ps = store.lookup_scripture(Some("WEB"), "Ps 100", 10).expect("ps100");
        assert_eq!(ps.len(), 2);
        let kw = store.lookup_scripture(Some("KJV"), "shepherd", 10).expect("kw");
        assert_eq!(kw.len(), 1);
        let empty = store.lookup_scripture(None, "", 5).expect("empty");
        assert_eq!(empty.len(), 5);
        assert!(parse_scripture_reference("invalid").is_none());

        // Build dual-translation slides
        let john_web = store.lookup_scripture(Some("WEB"), "jn 3:16-17", 10).expect("john web");
        let slides = build_scripture_slides(&john, &john_web, 1, true);
        assert_eq!(slides.len(), 2);
        assert!(slides[0].headline.contains("KJV / WEB"));
        assert_eq!(slides[0].elements.len(), 3); // primary + parallel + reference

        // Quick Lyrics with [Verse 1]/[Chorus], repeated chorus, max_lines=2, and dual-language '//'
        let raw = "[Verse 1]\nLine 1 // Linea 1\nLine 2 // Linea 2\nLine 3\n\n[Chorus]\nHoly holy\n\n[Chorus]\nHoly holy";
        let (parsed_slides, arrangements) = parse_quick_lyrics("Holy Song", raw, 2, Some("99999".into()));
        assert_eq!(parsed_slides.len(), 3); // Verse 1 split into 2 slides + 1 master Chorus slide
        assert_eq!(arrangements[0].group_order, vec!["Verse 1", "Chorus", "Chorus"]);
        assert_eq!(parsed_slides[0].elements.len(), 2); // primary + secondary translation

        let (empty_slides, _) = parse_quick_lyrics("Blank", "   ", 4, None);
        assert_eq!(empty_slides.len(), 1);

        // Overlays, Message Templates, Macros, and Scheduled Events persistence CRUD
        assert_eq!(store.list_overlays().expect("seeded overlays").len(), 2);
        let mut ov = default_overlays()[0].clone();
        ov.id = "ov-test".into();
        store.upsert_overlay(&ov).expect("upsert ov");
        assert!(store.delete_overlay("ov-test").expect("del ov"));

        assert_eq!(store.list_message_templates().expect("seeded msgs").len(), 2);
        let mut mt = default_message_templates()[0].clone();
        mt.id = "mt-test".into();
        store.upsert_message_template(&mt).expect("upsert mt");
        assert!(store.delete_message_template("mt-test").expect("del mt"));

        assert_eq!(store.list_macros().expect("seeded macros").len(), 2);
        store
            .upsert_macro(&MacroRecord {
                id: "m-test".into(),
                name: "Test Macro".into(),
                steps: vec!["blackout:on".into()],
                updated_at: Utc::now(),
            })
            .expect("upsert macro");
        assert!(store.delete_macro("m-test").expect("del macro"));

        assert_eq!(store.list_scheduled_events().expect("seeded sched").len(), 1);
        store
            .upsert_scheduled_event(&ScheduledEventRecord {
                id: "ev-test".into(),
                name: "Midweek".into(),
                trigger_hhmm: "19:00".into(),
                macro_id: "macro-worship-start".into(),
                enabled: true,
                updated_at: Utc::now(),
            })
            .expect("upsert sched");
        assert!(store.delete_scheduled_event("ev-test").expect("del sched"));
    }
}
