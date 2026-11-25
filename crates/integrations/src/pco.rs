//! Planning Center Services API v2 connector:
//! - Desktop OAuth 2.0 + PKCE (SHA-256 `S256` challenge, loopback redirect)
//! - JSON:API 1.0 `?include=items.arrangement.song` plan & arrangement parser
//! - Dynamic rate-limit governor (`100 req / 20s` via `X-PCO-API-Request-Rate-*` & `Retry-After`)

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const PCO_AUTH_ENDPOINT: &str = "https://api.planningcenteronline.com/oauth/authorize";
pub const PCO_TOKEN_ENDPOINT: &str = "https://api.planningcenteronline.com/oauth/token";
pub const PCO_SERVICES_BASE: &str = "https://api.planningcenteronline.com/services/v2";

#[derive(Debug, Error)]
pub enum PcoError {
    #[error("invalid OAuth configuration: {0}")]
    InvalidConfig(String),
    #[error("JSON:API parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// OAuth 2.0 PKCE session parameters for desktop loopback sign-in (zero embedded client secret).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcoOAuthPkceSession {
    pub client_id: String,
    pub redirect_uri: String,
    pub code_verifier: String,
    pub code_challenge: String,
    pub state: String,
}

impl PcoOAuthPkceSession {
    pub fn start(client_id: impl Into<String>, loopback_port: u16) -> Result<Self, PcoError> {
        let client_id = client_id.into();
        if client_id.trim().is_empty() {
            return Err(PcoError::InvalidConfig("client_id cannot be empty".into()));
        }
        let verifier = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        let digest = Sha256::digest(verifier.as_bytes());
        let challenge = URL_SAFE_NO_PAD.encode(digest);
        let state = uuid::Uuid::new_v4().simple().to_string();

        Ok(Self {
            client_id,
            redirect_uri: format!("http://127.0.0.1:{loopback_port}/pco/callback"),
            code_verifier: verifier,
            code_challenge: challenge,
            state,
        })
    }

    pub fn authorize_url(&self) -> String {
        format!(
            "{PCO_AUTH_ENDPOINT}?client_id={}&redirect_uri={}&response_type=code&scope=services&code_challenge={}&code_challenge_method=S256&state={}",
            self.client_id, self.redirect_uri, self.code_challenge, self.state
        )
    }
}

/// Dynamic rate-limit governor driven by Planning Center response headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcoRateLimitState {
    pub limit: u32,
    pub count: u32,
    pub period_secs: u32,
    pub retry_after_secs: Option<u32>,
}

impl Default for PcoRateLimitState {
    fn default() -> Self {
        Self {
            limit: 100,
            count: 0,
            period_secs: 20,
            retry_after_secs: None,
        }
    }
}

impl PcoRateLimitState {
    pub fn update_from_headers(&mut self, headers: &[(String, String)]) {
        self.retry_after_secs = None;
        for (k, v) in headers {
            match k.to_ascii_lowercase().as_str() {
                "x-pco-api-request-rate-limit" => {
                    if let Ok(n) = v.trim().parse::<u32>() {
                        self.limit = n;
                    }
                }
                "x-pco-api-request-rate-count" => {
                    if let Ok(n) = v.trim().parse::<u32>() {
                        self.count = n;
                    }
                }
                "x-pco-api-request-rate-period" => {
                    if let Ok(n) = v.trim().parse::<u32>() {
                        self.period_secs = n;
                    }
                }
                "retry-after" => {
                    self.retry_after_secs = v.trim().parse::<u32>().ok();
                }
                _ => {}
            }
        }
    }

    /// Suggested delay in milliseconds before dispatching the next request.
    pub fn recommended_backoff_ms(&self) -> u64 {
        if let Some(ra) = self.retry_after_secs {
            return u64::from(ra) * 1000;
        }
        if self.count >= self.limit.saturating_sub(5) {
            return u64::from(self.period_secs.max(1)) * 250;
        }
        0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcoPlanItem {
    pub id: String,
    pub title: String,
    pub item_type: String,
    pub song_ccli: Option<String>,
    pub arrangement_key: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcoPlanSummary {
    pub id: String,
    pub title: String,
    pub dates: String,
    pub items: Vec<PcoPlanItem>,
}

/// Builds a batched JSON:API URL pulling plans, items, arrangements, and songs in one round-trip.
pub fn build_pco_plans_url(service_type_id: &str) -> String {
    format!(
        "{PCO_SERVICES_BASE}/service_types/{service_type_id}/plans?filter=future&include=items.arrangement.song"
    )
}

/// Parses a Planning Center Services v2 JSON:API response with sideloaded `included` resources.
pub fn parse_pco_plans_json_api(json_body: &str) -> Result<Vec<PcoPlanSummary>, PcoError> {
    let root: serde_json::Value = serde_json::from_str(json_body)?;
    let mut included_map: HashMap<(String, String), &serde_json::Value> = HashMap::new();

    if let Some(included) = root.get("included").and_then(|v| v.as_array()) {
        for item in included {
            if let (Some(ty), Some(id)) = (
                item.get("type").and_then(|v| v.as_str()),
                item.get("id").and_then(|v| v.as_str()),
            ) {
                included_map.insert((ty.to_string(), id.to_string()), item);
            }
        }
    }

    let mut plans = Vec::new();
    let Some(data) = root.get("data").and_then(|v| v.as_array()) else {
        return Ok(plans);
    };

    for plan in data {
        let id = plan.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let attrs = plan.get("attributes");
        let title = attrs
            .and_then(|a| a.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled Service")
            .to_string();
        let dates = attrs
            .and_then(|a| a.get("dates"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        let mut items = Vec::new();
        if let Some(rel_items) = plan
            .pointer("/relationships/items/data")
            .and_then(|v| v.as_array())
        {
            for rel in rel_items {
                let item_id = rel.get("id").and_then(|v| v.as_str()).unwrap_or_default();
                if let Some(item_obj) = included_map.get(&("Item".to_string(), item_id.to_string())) {
                    let i_attrs = item_obj.get("attributes");
                    let song_id = item_obj
                        .pointer("/relationships/song/data/id")
                        .and_then(|v| v.as_str());
                    let arr_id = item_obj
                        .pointer("/relationships/arrangement/data/id")
                        .and_then(|v| v.as_str());

                    let song_ccli = song_id
                        .and_then(|sid| included_map.get(&("Song".to_string(), sid.to_string())))
                        .and_then(|s| s.pointer("/attributes/ccli_number"))
                        .and_then(|v| v.as_u64().map(|n| n.to_string()).or_else(|| v.as_str().map(String::from)));

                    let arrangement_key = arr_id
                        .and_then(|aid| {
                            included_map.get(&("Arrangement".to_string(), aid.to_string()))
                        })
                        .and_then(|a| a.pointer("/attributes/chord_chart_key"))
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    items.push(PcoPlanItem {
                        id: item_id.to_string(),
                        title: i_attrs
                            .and_then(|a| a.get("title"))
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        item_type: i_attrs
                            .and_then(|a| a.get("item_type"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("item")
                            .to_string(),
                        song_ccli,
                        arrangement_key,
                        description: i_attrs
                            .and_then(|a| a.get("description"))
                            .and_then(|v| v.as_str())
                            .map(String::from),
                    });
                }
            }
        }

        plans.push(PcoPlanSummary {
            id,
            title,
            dates,
            items,
        });
    }

    Ok(plans)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PcoAttachmentKind {
    LyricPdf,
    ChordChartPdf,
    SheetMusicPdf,
    Image,
    Video,
    Audio,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcoAttachment {
    pub id: String,
    pub filename: String,
    pub display_name: String,
    pub content_type: Option<String>,
    pub file_size: Option<u64>,
    pub kind: PcoAttachmentKind,
    pub context_label: String,
    pub open_endpoint: String,
    pub download_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedMediaFile {
    pub blake3_hash: String,
    pub disk_path: PathBuf,
    pub safe_file_name: String,
    pub display_name: String,
    pub media_kind: String,
    pub size_bytes: u64,
    pub deduplicated: bool,
}

pub fn build_pco_plan_attachments_url(service_type_id: &str, plan_id: &str) -> String {
    format!("{PCO_SERVICES_BASE}/service_types/{service_type_id}/plans/{plan_id}/attachments?per_page=100")
}

pub fn build_pco_item_attachments_url(
    service_type_id: &str,
    plan_id: &str,
    item_id: &str,
) -> String {
    format!(
        "{PCO_SERVICES_BASE}/service_types/{service_type_id}/plans/{plan_id}/items/{item_id}/attachments?per_page=100"
    )
}

pub fn build_pco_song_attachments_url(song_id: &str) -> String {
    format!("{PCO_SERVICES_BASE}/songs/{song_id}/attachments?per_page=100")
}

pub fn build_pco_attachment_open_url(attachment_id: &str) -> String {
    format!("{PCO_SERVICES_BASE}/attachments/{attachment_id}/open")
}

pub fn classify_attachment_kind(
    filename: &str,
    content_type: Option<&str>,
) -> PcoAttachmentKind {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    let lower_name = filename.to_ascii_lowercase();
    let ct = content_type.unwrap_or("").to_ascii_lowercase();

    if ext == "pdf" || lower_name.contains(".pdf") || ct.contains("application/pdf") {
        let is_sheet = [
            "sheet", "score", "piano", "satb", "vocal", "clef", "orchestr", "hymnal", "lead sheet",
            "lead_sheet", "leadsheet", "notation", "choir", "strings", "brass", "horn",
        ]
        .iter()
        .any(|k| lower_name.contains(k));
        if is_sheet {
            return PcoAttachmentKind::SheetMusicPdf;
        }
        let is_chord = [
            "chord", "chart", "nashville", "number", "capo", "guitar", "acoustic", "rhythm", "band",
        ]
        .iter()
        .any(|k| lower_name.contains(k));
        if is_chord {
            return PcoAttachmentKind::ChordChartPdf;
        }
        return PcoAttachmentKind::LyricPdf;
    }
    if matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" | "svg" | "tif" | "tiff"
    ) || ct.starts_with("image/")
    {
        return PcoAttachmentKind::Image;
    }
    if matches!(ext.as_str(), "mp4" | "mov" | "webm" | "mkv" | "m4v" | "avi")
        || ct.starts_with("video/")
    {
        return PcoAttachmentKind::Video;
    }
    if matches!(ext.as_str(), "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "opus")
        || ct.starts_with("audio/")
    {
        return PcoAttachmentKind::Audio;
    }
    PcoAttachmentKind::Other
}

/// Parses a Planning Center Services v2 `/attachments` JSON:API response into typed attachments.
pub fn parse_pco_attachments_json_api(
    json_body: &str,
    context_label: Option<&str>,
) -> Result<Vec<PcoAttachment>, PcoError> {
    let root: serde_json::Value = serde_json::from_str(json_body)?;
    let Some(data) = root.get("data").and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };
    let ctx = context_label
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Planning Center")
        .to_string();

    let mut out = Vec::new();
    for item in data {
        let id = item
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if id.is_empty() {
            continue;
        }
        let attrs = item.get("attributes");
        let filename = attrs
            .and_then(|a| a.get("filename"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("attachment.bin")
            .to_string();
        let display_name = attrs
            .and_then(|a| a.get("display_name"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&filename)
            .to_string();
        let content_type = attrs
            .and_then(|a| a.get("content_type"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let file_size = attrs
            .and_then(|a| a.get("file_size"))
            .and_then(|v| v.as_u64());
        let download_url = attrs
            .and_then(|a| a.get("url").or_else(|| a.get("attachment_url")))
            .and_then(|v| v.as_str())
            .filter(|s| s.starts_with("http"))
            .map(String::from);
        let kind = if filename.to_ascii_lowercase().ends_with(".pdf")
            || content_type
                .as_deref()
                .map(|c| c.to_ascii_lowercase().contains("application/pdf"))
                .unwrap_or(false)
        {
            classify_attachment_kind(&format!("{filename} {display_name}"), Some("application/pdf"))
        } else {
            classify_attachment_kind(&filename, content_type.as_deref())
        };

        out.push(PcoAttachment {
            open_endpoint: build_pco_attachment_open_url(&id),
            id,
            filename,
            display_name,
            content_type,
            file_size,
            kind,
            context_label: ctx.clone(),
            download_url,
        });
    }
    Ok(out)
}

/// Extracts the signed temporary download URL returned by `POST /services/v2/attachments/{id}/open`.
pub fn parse_pco_open_attachment_url(json_body: &str) -> Result<String, PcoError> {
    let root: serde_json::Value = serde_json::from_str(json_body)?;
    root.pointer("/data/attributes/attachment_url")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(String::from)
        .ok_or_else(|| {
            PcoError::InvalidConfig("PCO attachment open response missing attachment_url".into())
        })
}

fn sanitize_slug(raw: &str, max_len: usize) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in raw.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
        if out.len() >= max_len {
            break;
        }
    }
    out.trim_end_matches('-').to_string()
}

/// Generates a meaningful, collision-proof disk filename (`{context}__{stem}__{hash8}.{ext}`)
/// and human-readable display label (`{context} — {stem}.{ext}`) so media from different
/// presentations sharing the same original filename (e.g. `background.mp4`) NEVER overwrite each other.
pub fn build_safe_media_names(
    context_label: &str,
    original_filename: &str,
    blake3_hex: &str,
) -> (String, String) {
    let path = Path::new(original_filename);
    let raw_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("media");
    let raw_ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or_else(|| "bin".to_string());

    let ctx_slug = sanitize_slug(context_label, 36);
    let stem_slug = sanitize_slug(raw_stem, 48);
    let stem_slug = if stem_slug.is_empty() {
        "media".to_string()
    } else {
        stem_slug
    };
    let hash8 = &blake3_hex[..blake3_hex.len().min(8)];

    let safe_disk_name = if ctx_slug.is_empty() {
        format!("{stem_slug}__{hash8}.{raw_ext}")
    } else {
        format!("{ctx_slug}__{stem_slug}__{hash8}.{raw_ext}")
    };

    let clean_ctx = context_label.trim();
    let display_name = if clean_ctx.is_empty()
        || raw_stem
            .to_ascii_lowercase()
            .contains(&clean_ctx.to_ascii_lowercase())
    {
        format!("{raw_stem}.{raw_ext}")
    } else {
        format!("{clean_ctx} — {raw_stem}.{raw_ext}")
    };

    (safe_disk_name, display_name)
}

/// Content-addresses and writes imported media bytes into `dest_dir` with a meaningful,
/// collision-proof filename (`{context}__{stem}__{hash8}.{ext}`).
///
/// - Two distinct files with the same original filename (`background.mp4`) get different `hash8`
///   suffixes and NEVER overwrite each other.
/// - Two byte-identical files share the exact same `blake3_hash` and reuse the staged file cleanly.
pub fn stage_imported_media_bytes(
    dest_dir: &Path,
    context_label: &str,
    original_filename: &str,
    bytes: &[u8],
) -> Result<StagedMediaFile, PcoError> {
    if bytes.is_empty() {
        return Err(PcoError::InvalidConfig("imported media payload is empty".into()));
    }
    std::fs::create_dir_all(dest_dir)?;
    let blake3_hash = blake3::hash(bytes).to_hex().to_string();
    let (safe_file_name, display_name) =
        build_safe_media_names(context_label, original_filename, &blake3_hash);
    let disk_path = dest_dir.join(&safe_file_name);
    let deduplicated = disk_path.exists();
    if !deduplicated {
        std::fs::write(&disk_path, bytes)?;
    }
    let media_kind = core_library::infer_media_kind(&disk_path).to_string();

    Ok(StagedMediaFile {
        blake3_hash,
        disk_path,
        safe_file_name,
        display_name,
        media_kind,
        size_bytes: bytes.len() as u64,
        deduplicated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_pkce_oauth_url_and_tracks_pco_rate_limits() {
        assert!(PcoOAuthPkceSession::start("   ", 8900).is_err());

        let sess = PcoOAuthPkceSession::start("trueshow-byok-id", 8900).expect("pkce");
        let url = sess.authorize_url();
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("http://127.0.0.1:8900/pco/callback"));
        assert!(build_pco_plans_url("42").contains("include=items.arrangement.song"));

        let mut rl = PcoRateLimitState::default();
        assert_eq!(rl.recommended_backoff_ms(), 0);
        rl.update_from_headers(&[
            ("X-PCO-API-Request-Rate-Limit".into(), "100".into()),
            ("X-PCO-API-Request-Rate-Count".into(), "97".into()),
            ("X-PCO-API-Request-Rate-Period".into(), "20".into()),
        ]);
        assert!(rl.recommended_backoff_ms() > 0);

        rl.update_from_headers(&[("Retry-After".into(), "12".into())]);
        assert_eq!(rl.recommended_backoff_ms(), 12_000);
    }

    #[test]
    fn parses_pco_json_api_plans_and_rejects_corrupt_json() {
        let body = r#"{
            "data": [{
                "type": "Plan",
                "id": "p1",
                "attributes": { "title": "Sunday Gathering", "dates": "Nov 29, 2026" },
                "relationships": {
                    "items": { "data": [{ "type": "Item", "id": "i1" }] }
                }
            }],
            "included": [
                {
                    "type": "Item",
                    "id": "i1",
                    "attributes": { "title": "Amazing Grace", "item_type": "song", "description": "Acoustic intro" },
                    "relationships": {
                        "song": { "data": { "type": "Song", "id": "s1" } },
                        "arrangement": { "data": { "type": "Arrangement", "id": "a1" } }
                    }
                },
                { "type": "Song", "id": "s1", "attributes": { "ccli_number": 22025 } },
                { "type": "Arrangement", "id": "a1", "attributes": { "chord_chart_key": "G" } }
            ]
        }"#;

        let plans = parse_pco_plans_json_api(body).expect("parse pco");
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].title, "Sunday Gathering");
        assert_eq!(plans[0].items.len(), 1);
        assert_eq!(plans[0].items[0].song_ccli.as_deref(), Some("22025"));
        assert_eq!(plans[0].items[0].arrangement_key.as_deref(), Some("G"));

        assert!(parse_pco_plans_json_api("{bad json").is_err());
    }

    #[test]
    fn parses_pco_attachments_and_stages_collision_safe_media_without_overwriting() {
        assert!(build_pco_plan_attachments_url("10", "20").contains("/plans/20/attachments"));
        assert!(build_pco_item_attachments_url("10", "20", "30").contains("/items/30/attachments"));
        assert!(build_pco_song_attachments_url("99").contains("/songs/99/attachments"));

        let att_json = r#"{
            "data": [
                {
                    "type": "Attachment",
                    "id": "att-1",
                    "attributes": {
                        "filename": "Goodness Of God-lyrics.pdf",
                        "display_name": "Goodness Of God (Lyrics PDF)",
                        "content_type": "application/pdf",
                        "file_size": 48200
                    }
                },
                {
                    "type": "Attachment",
                    "id": "att-2",
                    "attributes": {
                        "filename": "background.mp4",
                        "content_type": "video/mp4",
                        "file_size": 1200000
                    }
                },
                {
                    "type": "Attachment",
                    "id": "att-3",
                    "attributes": {
                        "filename": "title-slide.png",
                        "content_type": "image/png"
                    }
                },
                {
                    "type": "Attachment",
                    "id": "att-4",
                    "attributes": {
                        "filename": "walkin.mp3",
                        "content_type": "audio/mpeg"
                    }
                }
            ]
        }"#;

        let list = parse_pco_attachments_json_api(att_json, Some("Sunday Service")).expect("parse");
        assert_eq!(list.len(), 4);
        assert_eq!(list[0].kind, PcoAttachmentKind::LyricPdf);
        assert_eq!(list[1].kind, PcoAttachmentKind::Video);
        assert_eq!(list[2].kind, PcoAttachmentKind::Image);
        assert_eq!(list[3].kind, PcoAttachmentKind::Audio);
        assert!(list[0].open_endpoint.ends_with("/attachments/att-1/open"));

        let open_json = r#"{"data":{"type":"AttachmentActivity","attributes":{"attachment_url":"https://s3.example.com/signed.pdf"}}}"#;
        assert_eq!(
            parse_pco_open_attachment_url(open_json).expect("open url"),
            "https://s3.example.com/signed.pdf"
        );
        assert!(parse_pco_open_attachment_url("{}").is_err());

        // Collision-safe staging: two different presentations both attach "background.mp4" with different bytes!
        let tmp_dir = std::env::temp_dir().join(format!("op-pco-media-{}", uuid::Uuid::new_v4()));
        let first = stage_imported_media_bytes(&tmp_dir, "Amazing Grace", "background.mp4", b"video-bytes-A")
            .expect("stage A");
        let second = stage_imported_media_bytes(&tmp_dir, "Sermon Opener", "background.mp4", b"video-bytes-B")
            .expect("stage B");
        let dup = stage_imported_media_bytes(&tmp_dir, "Amazing Grace", "background.mp4", b"video-bytes-A")
            .expect("stage dup");

        assert_ne!(first.disk_path, second.disk_path);
        assert_ne!(first.blake3_hash, second.blake3_hash);
        assert!(first.safe_file_name.starts_with("Amazing-Grace__background__"));
        assert!(second.safe_file_name.starts_with("Sermon-Opener__background__"));
        assert_eq!(first.display_name, "Amazing Grace — background.mp4");
        assert!(dup.deduplicated);
        assert_eq!(std::fs::read(&first.disk_path).expect("read A"), b"video-bytes-A");
        assert_eq!(std::fs::read(&second.disk_path).expect("read B"), b"video-bytes-B");
        assert!(stage_imported_media_bytes(&tmp_dir, "Empty", "a.png", b"").is_err());
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
