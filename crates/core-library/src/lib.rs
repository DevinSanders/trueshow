//! Local-first SQLite storage engine (WAL + FTS5) for OpenPresenter.
//!
//! Guarantees 100% offline Sunday-morning operation across songs, slide presentations,
//! service playlists, Bible verses, BLAKE3 content-addressed media assets, CCLI usage
//! audit logs, and CRDT delta persistence.

use chrono::{DateTime, Utc};
use core_scene::{Slide, SlideId};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SongRecord {
    pub id: String,
    pub title: String,
    pub author: String,
    pub copyright: String,
    pub ccli_number: Option<String>,
    pub key_signature: Option<String>,
    pub bpm: Option<f32>,
    pub lyrics_plain: String,
    pub slides: Vec<Slide>,
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

            CREATE TABLE IF NOT EXISTS songs (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                author TEXT NOT NULL DEFAULT '',
                copyright TEXT NOT NULL DEFAULT '',
                ccli_number TEXT,
                key_signature TEXT,
                bpm REAL,
                lyrics_plain TEXT NOT NULL DEFAULT '',
                slides_json TEXT NOT NULL,
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
            "#,
        )?;
        Ok(())
    }

    pub fn upsert_song(&self, song: &SongRecord) -> Result<(), LibraryError> {
        let slides_json = serde_json::to_string(&song.slides)?;
        self.conn.execute(
            r#"
            INSERT INTO songs (id, title, author, copyright, ccli_number, key_signature, bpm, lyrics_plain, slides_json, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                author = excluded.author,
                copyright = excluded.copyright,
                ccli_number = excluded.ccli_number,
                key_signature = excluded.key_signature,
                bpm = excluded.bpm,
                lyrics_plain = excluded.lyrics_plain,
                slides_json = excluded.slides_json,
                updated_at = excluded.updated_at
            "#,
            params![
                song.id,
                song.title,
                song.author,
                song.copyright,
                song.ccli_number,
                song.key_signature,
                song.bpm,
                song.lyrics_plain,
                slides_json,
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

    pub fn list_songs(&self) -> Result<Vec<SongRecord>, LibraryError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, author, copyright, ccli_number, key_signature, bpm, lyrics_plain, slides_json, updated_at FROM songs ORDER BY title COLLATE NOCASE ASC",
        )?;
        let rows = stmt.query_map([], row_to_song)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_song(&self, id: &str) -> Result<Option<SongRecord>, LibraryError> {
        self.conn
            .query_row(
                "SELECT id, title, author, copyright, ccli_number, key_signature, bpm, lyrics_plain, slides_json, updated_at FROM songs WHERE id = ?1",
                params![id],
                row_to_song,
            )
            .optional()
            .map_err(LibraryError::from)
    }

    /// Full-text search across song titles, authors, CCLI numbers, and lyrics using SQLite FTS5.
    pub fn search_songs(&self, query: &str, limit: usize) -> Result<Vec<SongRecord>, LibraryError> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return self.list_songs();
        }
        let fts_query = sanitize_fts_query(trimmed);
        let mut stmt = self.conn.prepare(
            r#"
            SELECT s.id, s.title, s.author, s.copyright, s.ccli_number, s.key_signature, s.bpm, s.lyrics_plain, s.slides_json, s.updated_at
            FROM songs_fts f
            JOIN songs s ON s.id = f.song_id
            WHERE songs_fts MATCH ?1
            ORDER BY rank
            LIMIT ?2
            "#,
        )?;
        let rows = stmt.query_map(params![fts_query, limit as i64], row_to_song)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
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
        let path = path.as_ref();
        let bytes = std::fs::read(path)?;
        let hash = blake3::hash(&bytes).to_hex().to_string();
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unnamed".to_string());

        let record = MediaAssetRecord {
            blake3_hash: hash,
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
        self.conn.execute(
            r#"
            INSERT INTO media_assets (blake3_hash, file_name, local_path, kind, width, height, duration_ms, has_alpha, size_bytes)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(blake3_hash) DO UPDATE SET
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

    pub fn list_media_assets(&self) -> Result<Vec<MediaAssetRecord>, LibraryError> {
        let mut stmt = self.conn.prepare(
            "SELECT blake3_hash, file_name, local_path, kind, width, height, duration_ms, has_alpha, size_bytes FROM media_assets ORDER BY file_name ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(MediaAssetRecord {
                blake3_hash: row.get(0)?,
                file_name: row.get(1)?,
                local_path: row.get(2)?,
                kind: row.get(3)?,
                width: row.get(4)?,
                height: row.get(5)?,
                duration_ms: row.get(6)?,
                has_alpha: row.get::<_, i32>(7)? != 0,
                size_bytes: row.get::<_, i64>(8)? as u64,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
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

    /// Seeds initial demo song records if the local database is empty.
    pub fn seed_demo_if_empty(&self) -> Result<(), LibraryError> {
        if !self.list_songs()?.is_empty() {
            return Ok(());
        }
        let deck = core_scene::demo_deck();
        let song = SongRecord {
            id: Uuid::from_u128(9001).to_string(),
            title: "Amazing Grace".to_string(),
            author: "John Newton".to_string(),
            copyright: "Public Domain".to_string(),
            ccli_number: Some("22025".to_string()),
            key_signature: Some("G".to_string()),
            bpm: Some(72.0),
            lyrics_plain: deck[1].plain_text(),
            slides: vec![deck[1].clone()],
            updated_at: Utc::now(),
        };
        self.upsert_song(&song)?;

        let playlist = PlaylistRecord {
            id: Uuid::from_u128(9101).to_string(),
            name: "Sunday Morning Service".to_string(),
            item_song_ids: vec![song.id],
            updated_at: Utc::now(),
        };
        self.upsert_playlist(&playlist)?;
        Ok(())
    }
}

fn row_to_song(row: &rusqlite::Row<'_>) -> rusqlite::Result<SongRecord> {
    let slides_json: String = row.get(8)?;
    let updated_str: String = row.get(9)?;
    let slides: Vec<Slide> = serde_json::from_str(&slides_json).unwrap_or_default();
    Ok(SongRecord {
        id: row.get(0)?,
        title: row.get(1)?,
        author: row.get(2)?,
        copyright: row.get(3)?,
        ccli_number: row.get(4)?,
        key_signature: row.get(5)?,
        bpm: row.get(6)?,
        lyrics_plain: row.get(7)?,
        slides,
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
        assert_eq!(all.len(), 1);
        assert!(store.get_song(&all[0].id).expect("get hit").is_some());
        assert!(store.get_song("nonexistent-id").expect("get miss").is_none());

        let playlists = store.list_playlists().expect("list playlists");
        assert_eq!(playlists.len(), 1);

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
        assert_eq!(store.search_bible(None, "loved", 5).expect("all trans").len(), 1);
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

        drop(store);
        let _ = std::fs::remove_dir_all(db_path.parent().expect("parent"));
    }
}
