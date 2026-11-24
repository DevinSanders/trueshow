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
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                match tag.as_str() {
                    "RVPresentationDocument" => {
                        for a in e.attributes().flatten() {
                            let k = String::from_utf8_lossy(a.key.as_ref());
                            let v = String::from_utf8_lossy(&a.value).into_owned();
                            if k == "CCLISongTitle" && !v.is_empty() {
                                title = v;
                            } else if k == "CCLISongNumber" && !v.is_empty() {
                                ccli_number = Some(v);
                            }
                        }
                    }
                    "RVSlideGrouping" => {
                        for a in e.attributes().flatten() {
                            if a.key.as_ref() == b"name" {
                                current_group = String::from_utf8_lossy(&a.value).into_owned();
                            }
                        }
                    }
                    "NSString" => {
                        for a in e.attributes().flatten() {
                            if a.key.as_ref() == b"rvXMLIvarName"
                                && (a.value.as_ref() == b"PlainText" || a.value.as_ref() == b"text")
                            {
                                in_text_node = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(t)) if in_text_node => {
                let text = t
                    .unescape()
                    .map_err(|e| ImporterError::Xml(e.to_string()))?
                    .trim()
                    .to_string();
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
            Ok(Event::End(e)) if e.name().as_ref() == b"NSString" => {
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
        return Err(ImporterError::InvalidInput("empty .pro protobuf stream".into()));
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
        return Err(ImporterError::InvalidInput("missing <song> root element".into()));
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
                b"title" => in_title = true,
                b"ccliNo" => in_ccli = true,
                b"verse" => {
                    for a in e.attributes().flatten() {
                        if a.key.as_ref() == b"name" {
                            current_verse = String::from_utf8_lossy(&a.value).into_owned();
                        }
                    }
                }
                b"lines" => {
                    in_lines = true;
                    lines_buf.clear();
                }
                _ => {}
            },
            Ok(Event::Text(t)) => {
                let txt = t
                    .unescape()
                    .map_err(|e| ImporterError::Xml(e.to_string()))?
                    .trim()
                    .to_string();
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
                b"title" => in_title = false,
                b"ccliNo" => in_ccli = false,
                b"lines" => {
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
pub fn import_plain_text_song(title: &str, text: &str) -> Result<PresentationImportResult, ImporterError> {
    if text.trim().is_empty() {
        return Err(ImporterError::InvalidInput("plain text input is empty".into()));
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
pub fn import_pptx_bytes(title: &str, pptx_bytes: &[u8]) -> Result<PresentationImportResult, ImporterError> {
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
                Ok(Event::Start(e)) if e.name().as_ref().ends_with(b":t") || e.name().as_ref() == b"t" => {
                    in_text = true;
                }
                Ok(Event::Text(t)) if in_text => {
                    let s = t
                        .unescape()
                        .map_err(|e| ImporterError::Xml(e.to_string()))?
                        .trim()
                        .to_string();
                    if !s.is_empty() {
                        texts.push(s);
                    }
                }
                Ok(Event::End(e)) if e.name().as_ref().ends_with(b":t") || e.name().as_ref() == b"t" => {
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

/// Builds headless LibreOffice conversion arguments with isolated user profile for visual `.pptx` PDF/SVG export.
pub fn build_libreoffice_pdf_args(input_pptx: &Path, output_dir: &Path, profile_dir: &Path) -> Vec<String> {
    vec![
        "--headless".to_string(),
        format!("-env:UserInstallation=file:///{}", profile_dir.to_string_lossy().replace('\\', "/")),
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
        let lo_args = build_libreoffice_pdf_args(Path::new("a.pptx"), Path::new("out"), Path::new("prof"));
        assert!(lo_args.contains(&"--headless".to_string()));
        let as_src = build_keynote_applescript(Path::new("/tmp/a.key"), Path::new("/tmp/a.pdf"));
        assert!(as_src.contains("tell application \"Keynote\""));
    }
}
