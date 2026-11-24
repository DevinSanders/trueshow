//! Clean-room worship ecosystems, migration importers, and local-first CRDT/S3 sync.
//!
//! Strictly excludes hardcoded API keys, copyrighted Bible bundles, and agent/MCP gating.

pub mod bible;
pub mod ccli;
pub mod importers;
pub mod pco;
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
    import_openlyrics_xml, import_plain_text_song, import_pptx_bytes, import_pro6_xml,
    import_pro7_protobuf,
};
pub use pco::{
    PcoOAuthPkceSession, PcoPlanSummary, PcoRateLimitState, build_pco_plans_url,
    parse_pco_plans_json_api,
};
pub use sync::{
    AssetSyncPlan, LocalCrdtLibrary, S3ProviderKind, S3SyncConfig, plan_blake3_asset_sync,
};
