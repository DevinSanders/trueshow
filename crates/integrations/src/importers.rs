//! Clean-room migration & document importers:
//! - ProPresenter 6 (`.pro6` XML)
//! - ProPresenter 7 (`.pro` Protobuf wire decoder via `prost` — original clean-room wire structs)
//! - OpenLyrics XML
//! - Smart Plain-Text stanza importer
//! - PowerPoint `.pptx` native OOXML (`zip` + `quick-xml`) + LibreOffice headless bridge
//! - macOS Keynote (`.key`) AppleScript export bridge

use core_scene::Slide;
use prost::Message;
use quick_xml::Reader;
use quick_xml::events::Event;
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read};
use std::path::Path;
use thiserror::Error;
use zip::ZipArchive;

#[derive(Debug, Error)]
pub enum ImporterError {
    #[error("empty or invalid input: {0}")]
    InvalidInput(String),
    #[error("XML parse error: {0}")]
    Xml(String),
    #[error("Protobuf decode error: {0}")]
    Protobuf(#[from] prost::DecodeError),
    #[error("ZIP/OOXML archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresentationImportResult {
    pub title: String,
    pub ccli_number: Option<String>,
    pub source_format: String,
    pub slides: Vec<Slide>,
}

/// Clean-room ProPresenter 6 (`.pro6` XML) importer extracting title, CCLI #, and slide text.
pub fn import_pro6_xml(xml: &str) -> Result<PresentationImportResult, ImporterError> {
    if !xml.contains("RVPresentationDocument") {
        return Err(ImporterError::InvalidInput(
            "missing RVPresentationDocument root element".into(),
        ));
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut title = "Imported Pro6 Presentation".to_string();
    let mut ccli_number = None;
    let mut current_group = "Slide".to_string();
    let mut in_text_node = false;
    let mut slides = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.name().as_ref() {
                "RVPresentationDocument" => {
                    for a in e.attributes().flatten() {
                        let k = a.key.as_ref();
                        let v = a.value.as_ref();
                        if k == "CCLISongTitle" && !v.is_empty() {
                            title = v.to_string();
                        } else if k == "CCLISongNumber" && !v.is_empty() {
                            ccli_number = Some(v.to_string());
                        }
                    }
                }
                "RVSlideGrouping" => {
                    for a in e.attributes().flatten() {
                        if a.key.as_ref() == "name" {
                            current_group = a.value.into_owned();
                        }
                    }
                }
                "NSString" => {
                    for a in e.attributes().flatten() {
                        if a.key.as_ref() == "rvXMLIvarName"
                            && (a.value.as_ref() == "PlainText" || a.value.as_ref() == "text")
                        {
                            in_text_node = true;
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Text(t)) if in_text_node => {
                let text = t.xml10_content().trim().to_string();
                if !text.is_empty() {
                    slides.push(core_library::slide_from_text(
                        &format!("{title} — {current_group}"),
                        &current_group,
                        &text,
                        ccli_number.clone(),
                    ));
                }
                in_text_node = false;
            }
            Ok(Event::End(e)) if e.name().as_ref() == "NSString" => {
                in_text_node = false;
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(ImporterError::Xml(err.to_string())),
            _ => {}
        }
    }

    Ok(PresentationImportResult {
        title,
        ccli_number,
        source_format: "ProPresenter 6 (.pro6)".into(),
        slides,
    })
}

// Clean-room minimal Protobuf wire structs for ProPresenter 7 `.pro` files.
// Unknown fields in newer Pro7 point releases are skipped automatically by `prost`.
#[derive(Clone, PartialEq, Message)]
pub struct Pro7PresentationWire {
    #[prost(string, tag = "2")]
    pub name: String,
    #[prost(message, optional, tag = "6")]
    pub ccli: Option<Pro7CcliWire>,
    #[prost(message, repeated, tag = "7")]
    pub cues: Vec<Pro7CueWire>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Pro7CcliWire {
    #[prost(string, tag = "1")]
    pub author: String,
    #[prost(string, tag = "2")]
    pub song_title: String,
    #[prost(uint32, tag = "4")]
    pub song_number: u32,
}

#[derive(Clone, PartialEq, Message)]
pub struct Pro7CueWire {
    #[prost(string, tag = "2")]
    pub name: String,
    #[prost(string, repeated, tag = "10")]
    pub slide_texts: Vec<String>,
}

/// Clean-room ProPresenter 7 (`.pro` binary Protobuf) decoder via `prost`.
pub fn import_pro7_protobuf(bytes: &[u8]) -> Result<PresentationImportResult, ImporterError> {
    if bytes.is_empty() {
        return Err(ImporterError::InvalidInput(
            "empty .pro protobuf stream".into(),
        ));
    }
    let decoded = Pro7PresentationWire::decode(bytes)?;
    let title = if !decoded.name.trim().is_empty() {
        decoded.name.clone()
    } else if let Some(c) = &decoded.ccli
        && !c.song_title.is_empty()
    {
        c.song_title.clone()
    } else {
        "Imported Pro7 Presentation".to_string()
    };
    let ccli_number = decoded
        .ccli
        .as_ref()
        .filter(|c| c.song_number > 0)
        .map(|c| c.song_number.to_string());

    let mut slides = Vec::new();
    for (idx, cue) in decoded.cues.iter().enumerate() {
        let group = if cue.name.trim().is_empty() {
            format!("Cue {}", idx + 1)
        } else {
            cue.name.clone()
        };
        let joined = cue.slide_texts.join("\n");
        if !joined.trim().is_empty() {
            slides.push(core_library::slide_from_text(
                &format!("{title} — {group}"),
                &group,
                &joined,
                ccli_number.clone(),
            ));
        }
    }

    Ok(PresentationImportResult {
        title,
        ccli_number,
        source_format: "ProPresenter 7 (.pro)".into(),
        slides,
    })
}

/// Clean-room OpenLyrics XML importer (`http://openlyrics.info/namespace/2009/song`).
pub fn import_openlyrics_xml(xml: &str) -> Result<PresentationImportResult, ImporterError> {
    if !xml.contains("<song") {
        return Err(ImporterError::InvalidInput(
            "missing <song> root element".into(),
        ));
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut title = "Untitled OpenLyrics Song".to_string();
    let mut ccli_number = None;
    let mut in_title = false;
    let mut in_ccli = false;
    let mut in_lines = false;
    let mut current_verse = "Verse".to_string();
    let mut lines_buf = Vec::new();
    let mut slides = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                "title" => in_title = true,
                "ccliNo" => in_ccli = true,
                "verse" => {
                    for a in e.attributes().flatten() {
                        if a.key.as_ref() == "name" {
                            current_verse = a.value.into_owned();
                        }
                    }
                }
                "lines" => {
                    in_lines = true;
                    lines_buf.clear();
                }
                _ => {}
            },
            Ok(Event::Text(t)) => {
                let txt = t.xml10_content().trim().to_string();
                if txt.is_empty() {
                    continue;
                }
                if in_title {
                    title = txt;
                } else if in_ccli {
                    ccli_number = Some(txt);
                } else if in_lines {
                    lines_buf.push(txt);
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                "title" => in_title = false,
                "ccliNo" => in_ccli = false,
                "lines" => {
                    in_lines = false;
                    if !lines_buf.is_empty() {
                        slides.push(core_library::slide_from_text(
                            &format!("{title} — {current_verse}"),
                            &current_verse,
                            &lines_buf.join("\n"),
                            ccli_number.clone(),
                        ));
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(err) => return Err(ImporterError::Xml(err.to_string())),
            _ => {}
        }
    }

    Ok(PresentationImportResult {
        title,
        ccli_number,
        source_format: "OpenLyrics XML".into(),
        slides,
    })
}

/// Smart plain-text lyric importer splitting `[Verse 1]` / `[Chorus]` and blank-line stanzas.
pub fn import_plain_text_song(
    title: &str,
    text: &str,
) -> Result<PresentationImportResult, ImporterError> {
    if text.trim().is_empty() {
        return Err(ImporterError::InvalidInput(
            "plain text input is empty".into(),
        ));
    }
    let mut group = "Verse 1".to_string();
    let mut stanza = Vec::new();
    let mut slides = Vec::new();

    for raw in text.lines() {
        let t = raw.trim();
        if t.is_empty() {
            if !stanza.is_empty() {
                slides.push(core_library::slide_from_text(
                    &format!("{title} — {group}"),
                    &group,
                    &stanza.join("\n"),
                    None,
                ));
                stanza.clear();
            }
            continue;
        }
        if t.starts_with('[') && t.ends_with(']') && t.len() > 2 {
            if !stanza.is_empty() {
                slides.push(core_library::slide_from_text(
                    &format!("{title} — {group}"),
                    &group,
                    &stanza.join("\n"),
                    None,
                ));
                stanza.clear();
            }
            group = t[1..t.len() - 1].trim().to_string();
        } else {
            stanza.push(t.to_string());
        }
    }
    if !stanza.is_empty() {
        slides.push(core_library::slide_from_text(
            &format!("{title} — {group}"),
            &group,
            &stanza.join("\n"),
            None,
        ));
    }

    Ok(PresentationImportResult {
        title: title.to_string(),
        ccli_number: None,
        source_format: "Plain Text".into(),
        slides,
    })
}

/// Native pure-Rust PowerPoint `.pptx` OOXML importer extracting editable text runs from `ppt/slides/slide*.xml`.
pub fn import_pptx_bytes(
    title: &str,
    pptx_bytes: &[u8],
) -> Result<PresentationImportResult, ImporterError> {
    let mut archive = ZipArchive::new(Cursor::new(pptx_bytes))?;
    let mut slide_paths: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            let name = archive.by_index(i).ok()?.name().to_string();
            if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    slide_paths.sort();

    if slide_paths.is_empty() {
        return Err(ImporterError::InvalidInput(
            "archive contains no ppt/slides/slide*.xml entries".into(),
        ));
    }

    let mut slides = Vec::new();
    for (idx, path) in slide_paths.iter().enumerate() {
        let mut entry = archive.by_name(path)?;
        let mut xml = String::new();
        entry.read_to_string(&mut xml)?;

        let mut reader = Reader::from_str(&xml);
        let mut in_text = false;
        let mut texts = Vec::new();
        loop {
            match reader.read_event() {
                Ok(Event::Start(e))
                    if e.name().as_ref().ends_with(":t") || e.name().as_ref() == "t" =>
                {
                    in_text = true;
                }
                Ok(Event::Text(t)) if in_text => {
                    let s = t.xml10_content().trim().to_string();
                    if !s.is_empty() {
                        texts.push(s);
                    }
                }
                Ok(Event::End(e))
                    if e.name().as_ref().ends_with(":t") || e.name().as_ref() == "t" =>
                {
                    in_text = false;
                }
                Ok(Event::Eof) => break,
                Err(err) => return Err(ImporterError::Xml(err.to_string())),
                _ => {}
            }
        }
        let body = if texts.is_empty() {
            format!("Slide {}", idx + 1)
        } else {
            texts.join("\n")
        };
        slides.push(core_library::slide_from_text(
            &format!("{title} — Slide {}", idx + 1),
            "PowerPoint",
            &body,
            None,
        ));
    }

    Ok(PresentationImportResult {
        title: title.to_string(),
        ccli_number: None,
        source_format: "PowerPoint OOXML (.pptx)".into(),
        slides,
    })
}

/// Clean-room importer for FreeShow `.show` JSON files (two-element array `[id, showObj]` or direct `{ name, slides, meta }`).
pub fn import_freeshow_json(json_str: &str) -> Result<PresentationImportResult, ImporterError> {
    let val: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| ImporterError::InvalidInput(format!("invalid FreeShow JSON: {e}")))?;

    let show_obj = if let Some(arr) = val.as_array() {
        arr.get(1)
            .or_else(|| arr.first())
            .and_then(|v| v.as_object())
    } else {
        val.as_object()
    }
    .ok_or_else(|| ImporterError::InvalidInput("missing FreeShow show object".into()))?;

    let title = show_obj
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Imported FreeShow Presentation")
        .to_string();

    let ccli_number = show_obj
        .get("meta")
        .and_then(|m| m.get("CCLI").or_else(|| m.get("ccli")))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);

    let slides_map = show_obj
        .get("slides")
        .and_then(|v| v.as_object())
        .ok_or_else(|| ImporterError::InvalidInput("FreeShow JSON missing slides map".into()))?;

    let mut slides = Vec::new();
    for (idx, (_slide_key, slide_val)) in slides_map.iter().enumerate() {
        let group = slide_val
            .get("group")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("Slide")
            .to_string();

        let mut lines_out = Vec::new();
        if let Some(items) = slide_val.get("items").and_then(|v| v.as_array()) {
            for item in items {
                if let Some(lines) = item.get("lines").and_then(|v| v.as_array()) {
                    for line in lines {
                        if let Some(runs) = line.get("text").and_then(|v| v.as_array()) {
                            let line_text: String = runs
                                .iter()
                                .filter_map(|r| r.get("value").and_then(|v| v.as_str()))
                                .collect();
                            let trimmed = line_text.trim();
                            if !trimmed.is_empty() {
                                lines_out.push(trimmed.to_string());
                            }
                        }
                    }
                }
            }
        }
        if !lines_out.is_empty() {
            slides.push(core_library::slide_from_text(
                &format!("{title} — {group} {}", idx + 1),
                &group,
                &lines_out.join("\n"),
                ccli_number.clone(),
            ));
        }
    }

    if slides.is_empty() {
        return Err(ImporterError::InvalidInput(
            "FreeShow JSON contained no text slides".into(),
        ));
    }

    Ok(PresentationImportResult {
        title,
        ccli_number,
        source_format: "FreeShow (.show JSON)".into(),
        slides,
    })
}

/// Clean-room importer for OpenSong XML files (`<song><title>`, `<ccli>`, `<lyrics>` with `[V1]`/`[C]` sections).
pub fn import_opensong_xml(xml: &str) -> Result<PresentationImportResult, ImporterError> {
    let mut reader = Reader::from_str(xml);
    let mut title = String::new();
    let mut ccli_number = None;
    let mut lyrics_raw = String::new();
    let mut in_title = false;
    let mut in_ccli = false;
    let mut in_lyrics = false;
    let mut found_song = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                "song" => found_song = true,
                "title" => in_title = true,
                "ccli" => in_ccli = true,
                "lyrics" => in_lyrics = true,
                _ => {}
            },
            Ok(Event::Text(t)) => {
                let txt = t.xml10_content().into_owned();
                if in_title {
                    title.push_str(txt.trim());
                } else if in_ccli && !txt.trim().is_empty() {
                    ccli_number = Some(txt.trim().to_string());
                } else if in_lyrics {
                    lyrics_raw.push_str(&txt);
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                "title" => in_title = false,
                "ccli" => in_ccli = false,
                "lyrics" => in_lyrics = false,
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(err) => return Err(ImporterError::Xml(err.to_string())),
            _ => {}
        }
    }

    if !found_song || lyrics_raw.trim().is_empty() {
        return Err(ImporterError::InvalidInput(
            "missing <song> or <lyrics> in OpenSong XML".into(),
        ));
    }
    if title.is_empty() {
        title = "Imported OpenSong".to_string();
    }

    let mut group = "Verse 1".to_string();
    let mut stanza = Vec::new();
    let mut slides = Vec::new();

    for raw in lyrics_raw.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('.') || trimmed.starts_with(';') {
            // Skip OpenSong chord lines (`.G  C  D`) and comment lines (`;note`).
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() > 2 {
            if !stanza.is_empty() {
                slides.push(core_library::slide_from_text(
                    &format!("{title} — {group}"),
                    &group,
                    &stanza.join("\n"),
                    ccli_number.clone(),
                ));
                stanza.clear();
            }
            let tag = &trimmed[1..trimmed.len() - 1];
            group = match tag.to_ascii_uppercase().as_str() {
                "C" | "C1" => "Chorus".into(),
                "B" | "B1" => "Bridge".into(),
                "P" | "P1" => "Pre-Chorus".into(),
                v if v.starts_with('V') => format!("Verse {}", v.trim_start_matches('V')),
                _ => tag.to_string(),
            };
        } else {
            // Strip leading single-space lyric indicator & OpenSong `||` slide break markers.
            let clean = trimmed.replace("||", "\n");
            stanza.push(clean);
        }
    }
    if !stanza.is_empty() {
        slides.push(core_library::slide_from_text(
            &format!("{title} — {group}"),
            &group,
            &stanza.join("\n"),
            ccli_number.clone(),
        ));
    }

    Ok(PresentationImportResult {
        title,
        ccli_number,
        source_format: "OpenSong XML".into(),
        slides,
    })
}

/// Imports one or more local image files (or an entire folder of exported slide images)
/// into a multi-slide presentation, staging every image with a collision-proof BLAKE3 filename.
pub fn import_images_as_presentation(
    title: &str,
    input_paths: &[std::path::PathBuf],
    staged_dir: &Path,
) -> Result<PresentationImportResult, ImporterError> {
    let mut resolved_images: Vec<std::path::PathBuf> = Vec::new();
    for p in input_paths {
        if p.is_dir() {
            let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(p)?
                .filter_map(|e| e.ok().map(|de| de.path()))
                .filter(|path| path.is_file() && is_supported_image_ext(path))
                .collect();
            entries.sort();
            resolved_images.extend(entries);
        } else if p.is_file() && is_supported_image_ext(p) {
            resolved_images.push(p.clone());
        }
    }

    if resolved_images.is_empty() {
        return Err(ImporterError::InvalidInput(
            "no supported image files (.png, .jpg, .jpeg, .bmp, .webp) found".into(),
        ));
    }

    let clean_title = if title.trim().is_empty() {
        "Imported Image Deck"
    } else {
        title.trim()
    };

    let mut slides = Vec::with_capacity(resolved_images.len());
    for (idx, img_path) in resolved_images.iter().enumerate() {
        let orig_name = img_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("slide.png");
        let bytes = std::fs::read(img_path)?;
        let staged =
            crate::pco::stage_imported_media_bytes(staged_dir, clean_title, orig_name, &bytes)
                .map_err(|e| ImporterError::InvalidInput(e.to_string()))?;

        let mut slide = core_library::slide_from_text(
            &format!("{clean_title} — Slide {}", idx + 1),
            "Image Slide",
            "",
            None,
        );
        slide.headline = staged.display_name.clone();
        slide.body = format!("Image Slide {}", idx + 1);
        slide.elements.clear();
        slide.media_path = Some(staged.disk_path.to_string_lossy().into_owned());
        slide.media_cue = Some(core_scene::SlideMediaCue {
            label: staged.display_name,
            media_path: staged.disk_path.to_string_lossy().into_owned(),
            kind: "image".to_string(),
            role: core_scene::MediaCueRole::Foreground,
            motion_style: core_scene::ShaderMotionStyle::AuroraWave,
            primary_color: slide.primary_color,
            secondary_color: slide.secondary_color,
            accent_color: slide.accent_color,
        });
        slides.push(slide);
    }

    Ok(PresentationImportResult {
        title: clean_title.to_string(),
        ccli_number: None,
        source_format: "Image Presentation".into(),
        slides,
    })
}

fn is_supported_image_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .is_some_and(|ext| {
            matches!(
                ext.as_str(),
                "png" | "jpg" | "jpeg" | "bmp" | "webp" | "gif"
            )
        })
}

/// Clean-room Apple Keynote (`.key`) ZIP archive importer.
///
/// Extracts editable text slides from `index.apxl` XML (when present) or stages embedded
/// slide preview/media images (`Data/*.png|*.jpg`, `preview.jpg`) as visual slides.
pub fn import_keynote_bytes(
    title: &str,
    keynote_bytes: &[u8],
    staged_dir: &Path,
) -> Result<PresentationImportResult, ImporterError> {
    let mut archive = ZipArchive::new(Cursor::new(keynote_bytes))?;
    let clean_title = if title.trim().is_empty() {
        "Imported Keynote Presentation"
    } else {
        title.trim()
    };

    // 1. Check for Keynote `index.apxl` XML containing editable slide text runs.
    let apxl_name = (0..archive.len()).find_map(|i| {
        let name = archive.by_index(i).ok()?.name().to_string();
        if name.eq_ignore_ascii_case("index.apxl") || name.ends_with("/index.apxl") {
            Some(name)
        } else {
            None
        }
    });

    if let Some(apxl) = apxl_name {
        let mut entry = archive.by_name(&apxl)?;
        let mut xml = String::new();
        entry.read_to_string(&mut xml)?;
        let slides = parse_keynote_apxl_slides(clean_title, &xml)?;
        if !slides.is_empty() {
            return Ok(PresentationImportResult {
                title: clean_title.to_string(),
                ccli_number: None,
                source_format: "Apple Keynote (.key XML)".into(),
                slides,
            });
        }
    }

    // 2. Fall back to embedded slide media/preview images inside modern `.key` ZIP archives.
    let mut image_entries: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            let name = archive.by_index(i).ok()?.name().to_string();
            let lower = name.to_ascii_lowercase();
            let is_img =
                lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg");
            let is_thumb = lower.contains("preview-micro") || lower.contains("thumbnail");
            if is_img && !is_thumb {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    image_entries.sort();

    if !image_entries.is_empty() {
        let mut slides = Vec::new();
        for (idx, entry_name) in image_entries.iter().enumerate() {
            let mut entry = archive.by_name(entry_name)?;
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            if bytes.is_empty() {
                continue;
            }
            let base_name = Path::new(entry_name)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("keynote-slide.jpg");
            let staged =
                crate::pco::stage_imported_media_bytes(staged_dir, clean_title, base_name, &bytes)
                    .map_err(|e| ImporterError::InvalidInput(e.to_string()))?;

            let mut slide = core_library::slide_from_text(
                &format!("{clean_title} — Slide {}", idx + 1),
                "Keynote",
                "",
                None,
            );
            slide.headline = staged.display_name.clone();
            slide.body = format!("Keynote Slide {}", idx + 1);
            slide.elements.clear();
            slide.media_path = Some(staged.disk_path.to_string_lossy().into_owned());
            slide.media_cue = Some(core_scene::SlideMediaCue {
                label: staged.display_name,
                media_path: staged.disk_path.to_string_lossy().into_owned(),
                kind: "image".to_string(),
                role: core_scene::MediaCueRole::Foreground,
                motion_style: core_scene::ShaderMotionStyle::AuroraWave,
                primary_color: slide.primary_color,
                secondary_color: slide.secondary_color,
                accent_color: slide.accent_color,
            });
            slides.push(slide);
        }
        if !slides.is_empty() {
            return Ok(PresentationImportResult {
                title: clean_title.to_string(),
                ccli_number: None,
                source_format: "Apple Keynote (.key Archive)".into(),
                slides,
            });
        }
    }

    Err(ImporterError::InvalidInput(
        "Keynote archive contained no readable index.apxl or slide images; export to PDF or PPTX in Keynote".into(),
    ))
}

fn parse_keynote_apxl_slides(title: &str, xml: &str) -> Result<Vec<Slide>, ImporterError> {
    let mut reader = Reader::from_str(xml);
    let mut in_slide = false;
    let mut in_p = false;
    let mut current_lines: Vec<String> = Vec::new();
    let mut slides = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let n = name.as_ref();
                if n.ends_with(":slide") || n == "slide" {
                    in_slide = true;
                    current_lines.clear();
                } else if in_slide && (n.ends_with(":p") || n == "p") {
                    in_p = true;
                }
            }
            Ok(Event::Text(t)) if in_slide && in_p => {
                let txt = t.xml10_content().trim().to_string();
                if !txt.is_empty() {
                    current_lines.push(txt);
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let n = name.as_ref();
                if n.ends_with(":p") || n == "p" {
                    in_p = false;
                } else if n.ends_with(":slide") || n == "slide" {
                    in_slide = false;
                    if !current_lines.is_empty() {
                        slides.push(core_library::slide_from_text(
                            &format!("{title} — Slide {}", slides.len() + 1),
                            "Keynote",
                            &current_lines.join("\n"),
                            None,
                        ));
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(ImporterError::Xml(err.to_string())),
            _ => {}
        }
    }
    Ok(slides)
}

/// Builds headless LibreOffice conversion arguments with isolated user profile for visual `.pptx` PDF/SVG export.
pub fn build_libreoffice_pdf_args(
    input_pptx: &Path,
    output_dir: &Path,
    profile_dir: &Path,
) -> Vec<String> {
    vec![
        "--headless".to_string(),
        format!(
            "-env:UserInstallation=file:///{}",
            profile_dir.to_string_lossy().replace('\\', "/")
        ),
        "--convert-to".to_string(),
        "pdf".to_string(),
        "--outdir".to_string(),
        output_dir.to_string_lossy().into_owned(),
        input_pptx.to_string_lossy().into_owned(),
    ]
}

/// Builds a macOS AppleScript snippet invoking user-installed Apple Keynote to export `.key` decks to PDF.
pub fn build_keynote_applescript(input_key: &Path, output_pdf: &Path) -> String {
    format!(
        r#"tell application "Keynote"
    set doc to open POSIX file "{}"
    export doc to POSIX file "{}" as PDF
    close doc saving no
end tell"#,
        input_key.to_string_lossy(),
        output_pdf.to_string_lossy()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    #[test]
    fn imports_pro6_pro7_openlyrics_plain_text_and_pptx_with_failure_paths() {
        // ProPresenter 6 XML
        let pro6 = r#"<RVPresentationDocument CCLISongTitle="How Great Thou Art" CCLISongNumber="14181">
            <RVSlideGrouping name="Chorus"><NSString rvXMLIvarName="PlainText">Then sings my soul, my Savior God to Thee</NSString></RVSlideGrouping>
        </RVPresentationDocument>"#;
        let r6 = import_pro6_xml(pro6).expect("pro6");
        assert_eq!(r6.title, "How Great Thou Art");
        assert_eq!(r6.ccli_number.as_deref(), Some("14181"));
        assert_eq!(r6.slides.len(), 1);
        assert!(import_pro6_xml("<other/>").is_err());

        // ProPresenter 7 Protobuf (clean-room wire encode/decode)
        let wire = Pro7PresentationWire {
            name: "10,000 Reasons".into(),
            ccli: Some(Pro7CcliWire {
                author: "Matt Redman".into(),
                song_title: "10,000 Reasons".into(),
                song_number: 6016351,
            }),
            cues: vec![Pro7CueWire {
                name: "Chorus".into(),
                slide_texts: vec!["Bless the Lord O my soul".into()],
            }],
        };
        let encoded = wire.encode_to_vec();
        let r7 = import_pro7_protobuf(&encoded).expect("pro7");
        assert_eq!(r7.title, "10,000 Reasons");
        assert_eq!(r7.ccli_number.as_deref(), Some("6016351"));
        assert_eq!(r7.slides.len(), 1);
        assert!(import_pro7_protobuf(&[]).is_err());

        // OpenLyrics XML
        let ol = r#"<song><properties><titles><title>Be Thou My Vision</title></titles><ccliNo>30639</ccliNo></properties><lyrics><verse name="v1"><lines>Be Thou my vision, O Lord of my heart</lines></verse></lyrics></song>"#;
        let rol = import_openlyrics_xml(ol).expect("openlyrics");
        assert_eq!(rol.title, "Be Thou My Vision");
        assert_eq!(rol.ccli_number.as_deref(), Some("30639"));
        assert_eq!(rol.slides.len(), 1);
        assert!(import_openlyrics_xml("<invalid/>").is_err());

        // Plain text
        let pt = "[Verse 1]\nLine A\nLine B\n\n[Chorus]\nLine C";
        let rpt = import_plain_text_song("My Song", pt).expect("plain");
        assert_eq!(rpt.slides.len(), 2);
        assert_eq!(rpt.slides[1].group, "Chorus");
        assert!(import_plain_text_song("Empty", "   ").is_err());

        // Native OOXML .pptx ZIP
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zw = zip::ZipWriter::new(&mut cursor);
            zw.start_file("ppt/slides/slide1.xml", SimpleFileOptions::default())
                .expect("start file");
            zw.write_all(b"<p:sld xmlns:a=\"a\" xmlns:p=\"p\"><a:t>Sermon Point One</a:t></p:sld>")
                .expect("write xml");
            zw.finish().expect("finish zip");
        }
        let rpptx = import_pptx_bytes("Sunday Sermon", cursor.get_ref()).expect("pptx");
        assert_eq!(rpptx.slides.len(), 1);
        assert!(rpptx.slides[0].plain_text().contains("Sermon Point One"));
        assert!(import_pptx_bytes("Corrupt", b"not-a-zip").is_err());

        // LibreOffice & Keynote bridges
        let lo_args =
            build_libreoffice_pdf_args(Path::new("a.pptx"), Path::new("out"), Path::new("prof"));
        assert!(lo_args.contains(&"--headless".to_string()));
        let as_src = build_keynote_applescript(Path::new("/tmp/a.key"), Path::new("/tmp/a.pdf"));
        assert!(as_src.contains("tell application \"Keynote\""));

        // FreeShow .show JSON (both array tuple and object formats + error paths)
        let freeshow_arr = r#"["show-1", {
            "name": "Way Maker",
            "meta": { "CCLI": "7115744" },
            "slides": {
                "s1": {
                    "group": "Chorus",
                    "items": [{ "lines": [{ "text": [{ "value": "Way maker, miracle worker" }] }] }]
                }
            }
        }]"#;
        let rfs = import_freeshow_json(freeshow_arr).expect("freeshow array");
        assert_eq!(rfs.title, "Way Maker");
        assert_eq!(rfs.ccli_number.as_deref(), Some("7115744"));
        assert_eq!(rfs.slides.len(), 1);
        assert!(rfs.slides[0].plain_text().contains("miracle worker"));

        let freeshow_obj = r#"{ "name": "Direct Show", "slides": { "s1": { "group": "Verse 1", "items": [{ "lines": [{ "text": [{ "value": "Hello sanctuary" }] }] }] } } }"#;
        assert_eq!(
            import_freeshow_json(freeshow_obj)
                .expect("freeshow obj")
                .slides
                .len(),
            1
        );
        assert!(import_freeshow_json("not-json").is_err());
        assert!(import_freeshow_json(r#"{"name":"Empty","slides":{}}"#).is_err());

        // OpenSong XML (with chord lines, comments, section tags, and error paths)
        let opensong = r#"<song><title>Cornerstone</title><ccli>6158927</ccli><lyrics>
; comment line
[V1]
.C        Am
 My hope is built on nothing less
[C]
 Christ alone cornerstone
[B]
 Through the storm He is Lord
[P]
 Ready now
[Tag]
 Amen||Forever
</lyrics></song>"#;
        let ros = import_opensong_xml(opensong).expect("opensong");
        assert_eq!(ros.title, "Cornerstone");
        assert_eq!(ros.ccli_number.as_deref(), Some("6158927"));
        assert_eq!(ros.slides.len(), 5);
        assert_eq!(ros.slides[0].group, "Verse 1");
        assert_eq!(ros.slides[1].group, "Chorus");
        assert!(import_opensong_xml("<other/>").is_err());

        // Image Presentation Importer (folder + single file)
        let img_dir = std::env::temp_dir().join(format!("op-img-imp-{}", uuid::Uuid::new_v4()));
        let stage_dir = std::env::temp_dir().join(format!("op-img-stage-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&img_dir).expect("mkdir");
        std::fs::write(img_dir.join("01_welcome.png"), b"\x89PNG\r\n\x1a\nfake1").expect("w1");
        std::fs::write(img_dir.join("02_sermon.jpg"), b"\xFF\xD8\xFFfake2").expect("w2");
        let rimg = import_images_as_presentation(
            "Sunday Slides",
            std::slice::from_ref(&img_dir),
            &stage_dir,
        )
        .expect("image deck");
        assert_eq!(rimg.slides.len(), 2);
        assert!(rimg.slides[0].media_cue.is_some());
        assert!(import_images_as_presentation("Empty", &[], &stage_dir).is_err());

        // Apple Keynote (.key) ZIP with index.apxl XML
        let mut key_xml_buf = Cursor::new(Vec::new());
        {
            let mut zw = zip::ZipWriter::new(&mut key_xml_buf);
            zw.start_file("index.apxl", SimpleFileOptions::default())
                .expect("start");
            zw.write_all(b"<key:presentation xmlns:key=\"k\" xmlns:sf=\"s\"><key:slide><sf:p>Keynote Slide One</sf:p></key:slide></key:presentation>").expect("write");
            zw.finish().expect("finish");
        }
        let rkey1 = import_keynote_bytes("Sermon Key", key_xml_buf.get_ref(), &stage_dir)
            .expect("keynote xml");
        assert_eq!(rkey1.slides.len(), 1);
        assert!(rkey1.slides[0].plain_text().contains("Keynote Slide One"));

        // Apple Keynote (.key) ZIP with embedded preview/slide images
        let mut key_img_buf = Cursor::new(Vec::new());
        {
            let mut zw = zip::ZipWriter::new(&mut key_img_buf);
            zw.start_file("Data/slide1.png", SimpleFileOptions::default())
                .expect("start");
            zw.write_all(b"\x89PNG\r\n\x1a\nkeyslide").expect("write");
            zw.finish().expect("finish");
        }
        let rkey2 = import_keynote_bytes("Visual Key", key_img_buf.get_ref(), &stage_dir)
            .expect("keynote img");
        assert_eq!(rkey2.slides.len(), 1);
        assert!(rkey2.slides[0].media_cue.is_some());
        assert!(import_keynote_bytes("Bad", b"not-a-zip", &stage_dir).is_err());

        let _ = std::fs::remove_dir_all(&img_dir);
        let _ = std::fs::remove_dir_all(&stage_dir);
    }
}
