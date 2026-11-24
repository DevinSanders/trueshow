//! Clean-room worship ecosystems, migration importers, and local-first CRDT/S3 sync.
//!
//! Strictly excludes hardcoded API keys, copyrighted Bible bundles, and agent/MCP gating.

pub mod bible;
pub mod ccli;
pub mod importers;
pub mod pco;
pub mod pdf_lyrics;
pub mod sync;

pub use bible::{
    API_BIBLE_HARD_EXPIRY_SECS, API_BIBLE_MAX_CONSECUTIVE_VERSES, API_BIBLE_REFRESH_WARN_SECS,
    ApiBibleByokConfig, BibleCacheStatus, BibleXmlFormat, evaluate_api_bible_cache,
    parse_bible_xml,
};
pub use ccli::{
    ImportedSong, export_ccli_usage_csv, parse_chordpro, parse_songselect_text,
    parse_songselect_usr,
};
pub use importers::{
    PresentationImportResult, build_keynote_applescript, build_libreoffice_pdf_args,
    import_freeshow_json, import_openlyrics_xml, import_opensong_xml, import_plain_text_song,
    import_pptx_bytes, import_pro6_xml, import_pro7_protobuf,
};
pub use pco::{
    PcoAttachment, PcoAttachmentKind, PcoOAuthPkceSession, PcoPlanSummary, PcoRateLimitState,
    StagedMediaFile, build_pco_attachment_open_url, build_pco_item_attachments_url,
    build_pco_plan_attachments_url, build_pco_plans_url, build_pco_song_attachments_url,
    build_safe_media_names, classify_attachment_kind, parse_pco_attachments_json_api,
    parse_pco_open_attachment_url, parse_pco_plans_json_api, stage_imported_media_bytes,
};
pub use pdf_lyrics::{
    ParsedLyricSheet, PdfLyricsError, extract_pdf_text, import_lyric_sheet_pdf,
    parse_lyric_sheet_text,
};
pub use sync::{
    AssetSyncPlan, LocalCrdtLibrary, S3ProviderKind, S3SyncConfig, plan_blake3_asset_sync,
};
