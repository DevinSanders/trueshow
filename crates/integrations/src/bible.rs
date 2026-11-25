//! Offline public-domain XML Bible importers (OSIS, Zefania, USFX, Beblia) and
//! BYOK API.Bible compliance validator (14-day refresh advisory, 30-day hard cache expiry,
//! 500-consecutive-verse safeguard, FUMS tracking, and mandatory attribution).

use core_library::BibleVerseRecord;
use quick_xml::Reader;
use quick_xml::events::Event;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const API_BIBLE_REFRESH_WARN_SECS: i64 = 14 * 86_400;
pub const API_BIBLE_HARD_EXPIRY_SECS: i64 = 30 * 86_400;
pub const API_BIBLE_MAX_CONSECUTIVE_VERSES: usize = 500;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BibleError {
    #[error("XML parse error: {0}")]
    Xml(String),
    #[error("BYOK API.Bible key cannot be empty")]
    MissingApiKey,
    #[error("requested {0} consecutive verses exceeds API.Bible 500-verse policy limit")]
    ConsecutiveVerseLimitExceeded(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BibleXmlFormat {
    Osis,
    Zefania,
    Usfx,
    Beblia,
}

/// Clean-room parser supporting OSIS, Zefania, USFX, and Beblia public-domain XML Bibles.
pub fn parse_bible_xml(
    xml: &str,
    translation: &str,
    attribution: &str,
    format: BibleXmlFormat,
) -> Result<Vec<BibleVerseRecord>, BibleError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut verses = Vec::new();
    let mut current_book = String::new();
    let mut current_chapter: u32 = 1;
    let mut current_verse: Option<u32> = None;
    let mut verse_text = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.name().as_ref().to_ascii_lowercase();
                match (format, name.as_str()) {
                    (BibleXmlFormat::Zefania, "biblebook")
                    | (BibleXmlFormat::Beblia, "book")
                    | (BibleXmlFormat::Usfx, "book") => {
                        for attr in e.attributes().flatten() {
                            let k = attr.key.as_ref().to_ascii_lowercase();
                            if matches!(k.as_str(), "bname" | "name" | "id" | "number") {
                                current_book = attr.value.into_owned();
                            }
                        }
                    }
                    (BibleXmlFormat::Zefania, "chapter") | (BibleXmlFormat::Beblia, "chapter") => {
                        for attr in e.attributes().flatten() {
                            let k = attr.key.as_ref().to_ascii_lowercase();
                            if matches!(k.as_str(), "cnumber" | "number")
                                && let Ok(c) = attr.value.parse::<u32>()
                            {
                                current_chapter = c;
                            }
                        }
                    }
                    (BibleXmlFormat::Osis, "verse") => {
                        for attr in e.attributes().flatten() {
                            let k = attr.key.as_ref().to_ascii_lowercase();
                            if k == "osisid" {
                                let parts: Vec<&str> = attr.value.split('.').collect();
                                if parts.len() == 3 {
                                    current_book = parts[0].to_string();
                                    current_chapter = parts[1].parse().unwrap_or(1);
                                    current_verse = parts[2].parse().ok();
                                    verse_text.clear();
                                }
                            }
                        }
                    }
                    (BibleXmlFormat::Zefania, "vers") | (BibleXmlFormat::Beblia, "verse") => {
                        for attr in e.attributes().flatten() {
                            let k = attr.key.as_ref().to_ascii_lowercase();
                            if matches!(k.as_str(), "vnumber" | "number") {
                                current_verse = attr.value.parse().ok();
                                verse_text.clear();
                            }
                        }
                    }
                    (BibleXmlFormat::Usfx, "v") => {
                        for attr in e.attributes().flatten() {
                            let k = attr.key.as_ref().to_ascii_lowercase();
                            if k == "id" {
                                current_verse = attr.value.parse().ok();
                                verse_text.clear();
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let name = e.name().as_ref().to_ascii_lowercase();
                if format == BibleXmlFormat::Usfx && name == "c" {
                    for attr in e.attributes().flatten() {
                        let k = attr.key.as_ref().to_ascii_lowercase();
                        if k == "id"
                            && let Ok(c) = attr.value.parse::<u32>()
                        {
                            current_chapter = c;
                        }
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if current_verse.is_some() {
                    let s = t.xml10_content();
                    if !verse_text.is_empty() {
                        verse_text.push(' ');
                    }
                    verse_text.push_str(s.trim());
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name().as_ref().to_ascii_lowercase();
                let is_verse_end = matches!(
                    (format, name.as_str()),
                    (BibleXmlFormat::Osis, "verse")
                        | (BibleXmlFormat::Zefania, "vers")
                        | (BibleXmlFormat::Beblia, "verse")
                        | (BibleXmlFormat::Usfx, "v")
                );
                if is_verse_end && let Some(vnum) = current_verse.take() {
                    let clean = verse_text.trim().to_string();
                    if !clean.is_empty() {
                        verses.push(BibleVerseRecord {
                            translation: translation.to_string(),
                            book: current_book.clone(),
                            chapter: current_chapter,
                            verse: vnum,
                            text: clean,
                            attribution: attribution.to_string(),
                            cached_at_epoch: None,
                        });
                    }
                    verse_text.clear();
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(BibleError::Xml(err.to_string())),
            _ => {}
        }
    }

    Ok(verses)
}

/// BYOK API.Bible configuration (user-owned API key stored in OS Keychain, never hardcoded).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiBibleByokConfig {
    pub api_key: String,
    pub base_url: String,
    pub fums_version: u8,
}

impl ApiBibleByokConfig {
    pub fn new(api_key: impl Into<String>) -> Result<Self, BibleError> {
        let key = api_key.into();
        if key.trim().is_empty() {
            return Err(BibleError::MissingApiKey);
        }
        Ok(Self {
            api_key: key,
            base_url: "https://api.scripture.api.bible/v1".to_string(),
            fums_version: 3,
        })
    }

    pub fn passage_url(
        &self,
        bible_id: &str,
        passage_id: &str,
        requested_verses: usize,
    ) -> Result<String, BibleError> {
        if requested_verses > API_BIBLE_MAX_CONSECUTIVE_VERSES {
            return Err(BibleError::ConsecutiveVerseLimitExceeded(requested_verses));
        }
        Ok(format!(
            "{}/bibles/{}/passages/{}?content-type=text&include-notes=false&include-titles=false&fums-version={}",
            self.base_url, bible_id, passage_id, self.fums_version
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BibleCacheStatus {
    Fresh,
    RefreshRecommended,
    ExpiredMustPurge,
}

/// Evaluates contractual API.Bible cache age (recommended refresh <= 14d, hard expiry at 30d).
pub fn evaluate_api_bible_cache(cached_at_epoch: i64, now_epoch: i64) -> BibleCacheStatus {
    let age = now_epoch.saturating_sub(cached_at_epoch);
    if age >= API_BIBLE_HARD_EXPIRY_SECS {
        BibleCacheStatus::ExpiredMustPurge
    } else if age >= API_BIBLE_REFRESH_WARN_SECS {
        BibleCacheStatus::RefreshRecommended
    } else {
        BibleCacheStatus::Fresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_four_public_domain_bible_xml_formats() {
        let osis = r#"<osis><verse osisID="Ps.100.1">Make a joyful noise.</verse></osis>"#;
        let v_osis =
            parse_bible_xml(osis, "KJV", "Public Domain", BibleXmlFormat::Osis).expect("osis");
        assert_eq!(v_osis.len(), 1);
        assert_eq!(v_osis[0].book, "Ps");
        assert_eq!(v_osis[0].chapter, 100);
        assert_eq!(v_osis[0].verse, 1);

        let zef = r#"<XMLBIBLE><BIBLEBOOK bname="John"><CHAPTER cnumber="3"><VERS vnumber="16">For God so loved the world.</VERS></CHAPTER></BIBLEBOOK></XMLBIBLE>"#;
        let v_zef = parse_bible_xml(zef, "WEB", "PD", BibleXmlFormat::Zefania).expect("zefania");
        assert_eq!(v_zef.len(), 1);
        assert_eq!(v_zef[0].book, "John");
        assert_eq!(v_zef[0].verse, 16);

        let beblia = r#"<bible><book number="Genesis"><chapter number="1"><verse number="1">In the beginning.</verse></chapter></book></bible>"#;
        let v_beb = parse_bible_xml(beblia, "ASV", "PD", BibleXmlFormat::Beblia).expect("beblia");
        assert_eq!(v_beb[0].text, "In the beginning.");

        let usfx = r#"<usfx><book id="ROM"><c id="8"/><v id="28">All things work together for good.</v></book></usfx>"#;
        let v_usfx = parse_bible_xml(usfx, "WEB", "PD", BibleXmlFormat::Usfx).expect("usfx");
        assert_eq!(v_usfx[0].chapter, 8);
        assert_eq!(v_usfx[0].verse, 28);

        // Failure path: malformed XML returns error
        assert!(
            parse_bible_xml(
                "<osis><verse></bad></osis>",
                "KJV",
                "PD",
                BibleXmlFormat::Osis
            )
            .is_err()
        );
    }

    #[test]
    fn enforces_byok_api_bible_key_verse_cap_and_30_day_cache_rules() {
        assert_eq!(
            ApiBibleByokConfig::new("   "),
            Err(BibleError::MissingApiKey)
        );
        let cfg = ApiBibleByokConfig::new("user-secret-key").expect("valid");
        let url = cfg
            .passage_url("de4e12af7f28f599-02", "JHN.3.16", 1)
            .expect("url");
        assert!(url.contains("fums-version=3"));

        assert_eq!(
            cfg.passage_url("id", "GEN.1", 501),
            Err(BibleError::ConsecutiveVerseLimitExceeded(501))
        );

        let now = 1_750_000_000;
        assert_eq!(
            evaluate_api_bible_cache(now - 3 * 86_400, now),
            BibleCacheStatus::Fresh
        );
        assert_eq!(
            evaluate_api_bible_cache(now - 15 * 86_400, now),
            BibleCacheStatus::RefreshRecommended
        );
        assert_eq!(
            evaluate_api_bible_cache(now - 31 * 86_400, now),
            BibleCacheStatus::ExpiredMustPurge
        );
    }
}
