//! Local-first CRDT metadata sync (`loro`) + pluggable S3-compatible object storage
//! (Cloudflare R2, AWS S3, MinIO) BLAKE3 media asset sync planner.

use loro::{ExportMode, LoroDoc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("CRDT error: {0}")]
    Crdt(String),
    #[error("invalid S3 storage configuration: {0}")]
    InvalidS3Config(String),
}

/// Local-first collaborative show/library CRDT document powered by MIT-licensed `loro`.
pub struct LocalCrdtLibrary {
    doc: LoroDoc,
}

impl Default for LocalCrdtLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalCrdtLibrary {
    pub fn new() -> Self {
        Self {
            doc: LoroDoc::new(),
        }
    }

    /// Sets or updates a song title inside the replicated `songs` map container.
    pub fn put_song_title(&self, song_id: &str, title: &str) -> Result<(), SyncError> {
        let map = self.doc.get_map("songs");
        map.insert(song_id, title)
            .map_err(|e| SyncError::Crdt(e.to_string()))?;
        self.doc.commit();
        Ok(())
    }

    /// Appends a song ID to the collaborative service playlist list container.
    pub fn push_playlist_item(&self, song_id: &str) -> Result<(), SyncError> {
        let list = self.doc.get_list("sunday_playlist");
        list.push(song_id)
            .map_err(|e| SyncError::Crdt(e.to_string()))?;
        self.doc.commit();
        Ok(())
    }

    pub fn export_snapshot(&self) -> Result<Vec<u8>, SyncError> {
        self.doc
            .export(ExportMode::Snapshot)
            .map_err(|e| SyncError::Crdt(e.to_string()))
    }

    pub fn import_bytes(&self, bytes: &[u8]) -> Result<(), SyncError> {
        if bytes.is_empty() {
            return Err(SyncError::Crdt("empty CRDT payload".into()));
        }
        self.doc
            .import(bytes)
            .map_err(|e| SyncError::Crdt(e.to_string()))?;
        Ok(())
    }

    pub fn song_titles(&self) -> Vec<(String, String)> {
        let val = self.doc.get_map("songs").get_deep_value();
        let mut out = Vec::new();
        if let loro::LoroValue::Map(m) = val {
            for (k, v) in m.iter() {
                if let loro::LoroValue::String(s) = v {
                    out.push((k.clone(), s.to_string()));
                }
            }
        }
        out.sort();
        out
    }

    pub fn playlist_items(&self) -> Vec<String> {
        let val = self.doc.get_list("sunday_playlist").get_deep_value();
        let mut out = Vec::new();
        if let loro::LoroValue::List(items) = val {
            for item in items.iter() {
                if let loro::LoroValue::String(s) = item {
                    out.push(s.to_string());
                }
            }
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum S3ProviderKind {
    CloudflareR2,
    AwsS3,
    Minio,
}

/// Pluggable BYOK S3-compatible object storage configuration for large 4K video/media assets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct S3SyncConfig {
    pub provider: S3ProviderKind,
    pub endpoint_url: String,
    pub bucket: String,
    pub region: String,
    pub prefix: String,
}

impl S3SyncConfig {
    pub fn validate(&self) -> Result<(), SyncError> {
        if !self.endpoint_url.starts_with("https://") && !self.endpoint_url.starts_with("http://") {
            return Err(SyncError::InvalidS3Config(
                "endpoint_url must start with http:// or https://".into(),
            ));
        }
        if self.bucket.trim().is_empty() {
            return Err(SyncError::InvalidS3Config("bucket cannot be empty".into()));
        }
        Ok(())
    }

    pub fn object_key_for_blake3(&self, blake3_hash: &str) -> String {
        let clean_prefix = self.prefix.trim_matches('/');
        if clean_prefix.is_empty() {
            format!("media/{blake3_hash}")
        } else {
            format!("{clean_prefix}/media/{blake3_hash}")
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetSyncPlan {
    pub to_upload: Vec<String>,
    pub to_download: Vec<String>,
}

/// Computes content-addressed BLAKE3 media deltas between local SQLite manifest and S3 bucket.
pub fn plan_blake3_asset_sync(local_hashes: &[String], remote_hashes: &[String]) -> AssetSyncPlan {
    let local_set: HashSet<&str> = local_hashes.iter().map(String::as_str).collect();
    let remote_set: HashSet<&str> = remote_hashes.iter().map(String::as_str).collect();

    let mut to_upload: Vec<String> = local_set
        .difference(&remote_set)
        .map(|s| (*s).to_string())
        .collect();
    let mut to_download: Vec<String> = remote_set
        .difference(&local_set)
        .map(|s| (*s).to_string())
        .collect();
    to_upload.sort();
    to_download.sort();
    AssetSyncPlan {
        to_upload,
        to_download,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_divergent_offline_loro_crdt_replicas_without_conflicts() {
        let booth_mac = LocalCrdtLibrary::new();
        let office_pc = LocalCrdtLibrary::new();

        // Both machines edit offline independently
        booth_mac.put_song_title("s1", "Amazing Grace").expect("put 1");
        booth_mac.push_playlist_item("s1").expect("push 1");

        office_pc.put_song_title("s2", "Great Is Thy Faithfulness").expect("put 2");
        office_pc.push_playlist_item("s2").expect("push 2");

        let snap_mac = booth_mac.export_snapshot().expect("export mac");
        let snap_pc = office_pc.export_snapshot().expect("export pc");

        // Reconnect and merge bidirectionally
        booth_mac.import_bytes(&snap_pc).expect("merge pc -> mac");
        office_pc.import_bytes(&snap_mac).expect("merge mac -> pc");

        assert_eq!(booth_mac.song_titles(), office_pc.song_titles());
        assert_eq!(booth_mac.song_titles().len(), 2);
        assert_eq!(booth_mac.playlist_items().len(), 2);
        assert_eq!(booth_mac.playlist_items(), office_pc.playlist_items());

        // Failure paths: empty or corrupt payload rejected
        assert!(booth_mac.import_bytes(&[]).is_err());
        assert!(booth_mac.import_bytes(b"corrupt-crdt").is_err());
    }

    #[test]
    fn validates_s3_r2_config_and_plans_blake3_media_sync() {
        let cfg = S3SyncConfig {
            provider: S3ProviderKind::CloudflareR2,
            endpoint_url: "https://acct.r2.cloudflarestorage.com".into(),
            bucket: "church-media".into(),
            region: "auto".into(),
            prefix: "campus-north/".into(),
        };
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.object_key_for_blake3("abc123"), "campus-north/media/abc123");

        let bad_url = S3SyncConfig {
            endpoint_url: "ftp://invalid".into(),
            ..cfg.clone()
        };
        assert!(bad_url.validate().is_err());

        let bad_bucket = S3SyncConfig {
            bucket: "  ".into(),
            ..cfg
        };
        assert!(bad_bucket.validate().is_err());

        let plan = plan_blake3_asset_sync(
            &["hash_a".into(), "hash_b".into()],
            &["hash_b".into(), "hash_c".into()],
        );
        assert_eq!(plan.to_upload, vec!["hash_a".to_string()]);
        assert_eq!(plan.to_download, vec!["hash_c".to_string()]);
    }
}
