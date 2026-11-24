//! Clean-room Lyric Sheet & Chord Chart PDF parser (SongSelect / Planning Center PDFs).
//!
//! Extracts text from standard uncompressed and `/FlateDecode` (zlib) PDF content streams
//! without external binaries, parses worship section headers, strips CCLI license footers
//! and optional chord-chart lines, and builds editable [`Slide`]s + [`Arrangement`]s.

use core_library::slide_from_text;
use core_scene::{
    Arrangement, ChordChartDocument, ChordChartLine, ChordChartSection, SheetMusicDocument,
    SheetMusicPage, Slide,
};
use flate2::read::{DeflateDecoder, ZlibDecoder};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PdfLyricsError {
    #[error("invalid PDF file: {0}")]
    InvalidPdf(String),
    #[error("no readable lyric text could be extracted from PDF")]
    NoTextExtracted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedLyricSheet {
    pub title: String,
    pub author: String,
    pub copyright: String,
    pub ccli_number: Option<String>,
    pub key_signature: Option<String>,
    #[serde(default)]
    pub tempo_bpm: Option<f32>,
    #[serde(default)]
    pub time_signature: Option<String>,
    pub slides: Vec<Slide>,
    pub arrangements: Vec<Arrangement>,
    #[serde(default)]
    pub chord_chart: Option<ChordChartDocument>,
    pub raw_text_preview: String,
}

/// Extracts readable UTF-8 text lines from a PDF byte buffer (supports both uncompressed
/// and `/FlateDecode` zlib-compressed content streams, literal strings, hex strings, `Tj`, `TJ`,
/// `'`, `"`, `T*`, `Td`/`TD`/`Tm` positioning).
pub fn extract_pdf_text(pdf_bytes: &[u8]) -> Result<String, PdfLyricsError> {
    if pdf_bytes.len() < 8 || !pdf_bytes.starts_with(b"%PDF-") {
        return Err(PdfLyricsError::InvalidPdf(
            "missing %PDF- header".to_string(),
        ));
    }

    let mut extracted = String::new();
    let mut pos = 0usize;

    while let Some(stream_rel) = find_bytes(&pdf_bytes[pos..], b"stream") {
        let stream_kw = pos + stream_rel;
        // Look back up to 512 bytes for the enclosing stream dictionary to detect /FlateDecode.
        let dict_start = stream_kw.saturating_sub(512);
        let dict_slice = &pdf_bytes[dict_start..stream_kw];
        let is_flate = find_bytes(dict_slice, b"FlateDecode").is_some();

        // Per PDF spec, `stream` is followed by CRLF or LF.
        let mut data_start = stream_kw + b"stream".len();
        if data_start < pdf_bytes.len() && pdf_bytes[data_start] == b'\r' {
            data_start += 1;
        }
        if data_start < pdf_bytes.len() && pdf_bytes[data_start] == b'\n' {
            data_start += 1;
        } else {
            // False positive word "stream" inside metadata/text; continue scanning.
            pos = stream_kw + 6;
            continue;
        }

        let Some(end_rel) = find_bytes(&pdf_bytes[data_start..], b"endstream") else {
            break;
        };
        let mut data_end = data_start + end_rel;
        while data_end > data_start
            && (pdf_bytes[data_end - 1] == b'\n' || pdf_bytes[data_end - 1] == b'\r')
        {
            data_end -= 1;
        }

        let raw_stream = &pdf_bytes[data_start..data_end];
        let decoded_buf: Vec<u8>;
        let content: &[u8] = if is_flate {
            match decompress_flate(raw_stream) {
                Some(buf) => {
                    decoded_buf = buf;
                    &decoded_buf
                }
                None => {
                    pos = data_start + end_rel + b"endstream".len();
                    continue;
                }
            }
        } else {
            raw_stream
        };

        // Only parse PDF page content streams containing text blocks (`BT` ... `ET`) or `Tj`/`TJ`.
        if find_bytes(content, b"Tj").is_some() || find_bytes(content, b"TJ").is_some() {
            let chunk = parse_pdf_content_stream(content);
            if !chunk.trim().is_empty() {
                if !extracted.is_empty() {
                    extracted.push('\n');
                }
                extracted.push_str(chunk.trim());
            }
        }

        pos = data_start + end_rel + b"endstream".len();
    }

    if extracted.trim().is_empty() {
        return Err(PdfLyricsError::NoTextExtracted);
    }
    Ok(extracted)
}

fn decompress_flate(input: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut zlib = ZlibDecoder::new(input);
    if zlib.read_to_end(&mut out).is_ok() && !out.is_empty() {
        return Some(out);
    }
    out.clear();
    let mut raw = DeflateDecoder::new(input);
    if raw.read_to_end(&mut out).is_ok() && !out.is_empty() {
        return Some(out);
    }
    None
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn parse_pdf_content_stream(stream: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0usize;
    let len = stream.len();
    let mut last_string: Option<String> = None;
    let mut pending_nums: Vec<f32> = Vec::new();
    let mut last_tm_y: Option<f32> = None;

    while i < len {
        let b = stream[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // Comment
        if b == b'%' {
            while i < len && stream[i] != b'\n' && stream[i] != b'\r' {
                i += 1;
            }
            continue;
        }
        // Literal string (...)
        if b == b'(' {
            let (s, next_i) = read_pdf_literal_string(stream, i);
            last_string = Some(s);
            i = next_i;
            continue;
        }
        // Hex string <...> (but not dictionary <<)
        if b == b'<' && i + 1 < len && stream[i + 1] != b'<' {
            let (s, next_i) = read_pdf_hex_string(stream, i);
            last_string = Some(s);
            i = next_i;
            continue;
        }
        // Array [...] (used by TJ operator)
        if b == b'[' {
            let (joined, next_i) = read_pdf_tj_array(stream, i);
            last_string = Some(joined);
            i = next_i;
            continue;
        }
        // Number token
        if b.is_ascii_digit() || b == b'-' || b == b'+' || b == b'.' {
            let start = i;
            i += 1;
            while i < len && (stream[i].is_ascii_digit() || stream[i] == b'.') {
                i += 1;
            }
            if let Ok(tok) = std::str::from_utf8(&stream[start..i])
                && let Ok(val) = tok.parse::<f32>()
            {
                pending_nums.push(val);
            }
            continue;
        }
        // Quote operators ' and "
        if b == b'\'' || b == b'"' {
            if let Some(s) = last_string.take() {
                if !out.ends_with('\n') && !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&s);
            }
            pending_nums.clear();
            i += 1;
            continue;
        }
        // Identifier / operator token
        if b.is_ascii_alphabetic() || b == b'/' || b == b'*' {
            let start = i;
            i += 1;
            while i < len
                && (stream[i].is_ascii_alphanumeric()
                    || stream[i] == b'*'
                    || stream[i] == b'_'
                    || stream[i] == b'-')
            {
                i += 1;
            }
            let op = std::str::from_utf8(&stream[start..i]).unwrap_or("");
            match op {
                "Tj" | "TJ" => {
                    if let Some(s) = last_string.take() {
                        out.push_str(&s);
                    }
                    pending_nums.clear();
                }
                "T*" => {
                    if !out.ends_with('\n') && !out.is_empty() {
                        out.push('\n');
                    }
                    pending_nums.clear();
                }
                "Td" | "TD" => {
                    if let Some(&dy) = pending_nums.last()
                        && dy.abs() > 2.0
                        && !out.ends_with('\n')
                        && !out.is_empty()
                    {
                        out.push('\n');
                    }
                    pending_nums.clear();
                }
                "Tm" => {
                    if let Some(&y) = pending_nums.last() {
                        if let Some(prev_y) = last_tm_y
                            && (y - prev_y).abs() > 2.0
                            && !out.ends_with('\n')
                            && !out.is_empty()
                        {
                            out.push('\n');
                        }
                        last_tm_y = Some(y);
                    }
                    pending_nums.clear();
                }
                "ET" => {
                    if !out.ends_with('\n') && !out.is_empty() {
                        out.push('\n');
                    }
                    last_tm_y = None;
                    pending_nums.clear();
                }
                _ => {
                    pending_nums.clear();
                }
            }
            continue;
        }
        i += 1;
    }

    out
}

fn read_pdf_literal_string(stream: &[u8], start: usize) -> (String, usize) {
    let mut i = start + 1;
    let mut depth = 1usize;
    let mut bytes = Vec::new();
    while i < stream.len() && depth > 0 {
        let b = stream[i];
        if b == b'\\' && i + 1 < stream.len() {
            i += 1;
            match stream[i] {
                b'n' => bytes.push(b'\n'),
                b'r' => bytes.push(b'\r'),
                b't' => bytes.push(b'\t'),
                b'(' => bytes.push(b'('),
                b')' => bytes.push(b')'),
                b'\\' => bytes.push(b'\\'),
                b'\n' | b'\r' => {}
                d if (b'0'..=b'7').contains(&d) => {
                    let mut val = (d - b'0') as u32;
                    for _ in 0..2 {
                        if i + 1 < stream.len() && (b'0'..=b'7').contains(&stream[i + 1]) {
                            i += 1;
                            val = val * 8 + (stream[i] - b'0') as u32;
                        } else {
                            break;
                        }
                    }
                    bytes.push((val & 0xFF) as u8);
                }
                other => bytes.push(other),
            }
            i += 1;
            continue;
        }
        if b == b'(' {
            depth += 1;
            bytes.push(b);
        } else if b == b')' {
            depth -= 1;
            if depth > 0 {
                bytes.push(b);
            }
        } else {
            bytes.push(b);
        }
        i += 1;
    }
    (decode_pdf_bytes_to_string(&bytes), i)
}

fn read_pdf_hex_string(stream: &[u8], start: usize) -> (String, usize) {
    let mut i = start + 1;
    let mut hex_digits = Vec::new();
    while i < stream.len() && stream[i] != b'>' {
        if stream[i].is_ascii_hexdigit() {
            hex_digits.push(stream[i]);
        }
        i += 1;
    }
    if i < stream.len() && stream[i] == b'>' {
        i += 1;
    }
    if !hex_digits.len().is_multiple_of(2) {
        hex_digits.push(b'0');
    }
    let mut raw = Vec::with_capacity(hex_digits.len() / 2);
    for pair in hex_digits.as_chunks::<2>().0 {
        if let Ok(s) = std::str::from_utf8(pair)
            && let Ok(v) = u8::from_str_radix(s, 16)
        {
            raw.push(v);
        }
    }
    (decode_pdf_bytes_to_string(&raw), i)
}

fn decode_pdf_bytes_to_string(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let u16s: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&u16s);
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

fn read_pdf_tj_array(stream: &[u8], start: usize) -> (String, usize) {
    let mut i = start + 1;
    let mut out = String::new();
    while i < stream.len() && stream[i] != b']' {
        let b = stream[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if b == b'(' {
            let (s, next_i) = read_pdf_literal_string(stream, i);
            out.push_str(&s);
            i = next_i;
            continue;
        }
        if b == b'<' {
            let (s, next_i) = read_pdf_hex_string(stream, i);
            out.push_str(&s);
            i = next_i;
            continue;
        }
        if b == b'-' || b.is_ascii_digit() || b == b'.' {
            let num_start = i;
            i += 1;
            while i < stream.len() && (stream[i].is_ascii_digit() || stream[i] == b'.') {
                i += 1;
            }
            if let Ok(tok) = std::str::from_utf8(&stream[num_start..i])
                && let Ok(kern) = tok.parse::<f32>()
                && kern < -150.0
                && !out.ends_with(' ')
                && !out.is_empty()
            {
                out.push(' ');
            }
            continue;
        }
        i += 1;
    }
    if i < stream.len() && stream[i] == b']' {
        i += 1;
    }
    (out, i)
}

fn detect_section_header(line: &str) -> Option<String> {
    let clean = line
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches(':')
        .trim();
    if clean.is_empty() || clean.len() > 28 {
        return None;
    }
    let lower = clean.to_ascii_lowercase();
    let prefixes = [
        "verse",
        "chorus",
        "pre-chorus",
        "prechorus",
        "bridge",
        "tag",
        "intro",
        "outro",
        "ending",
        "interlude",
        "turnaround",
        "instrumental",
        "misc",
        "refrain",
        "vamp",
        "coda",
    ];
    for p in prefixes {
        if lower == p
            || lower.starts_with(&format!("{p} "))
            || (lower.starts_with(p)
                && lower[p.len()..].chars().all(|c| c.is_ascii_digit() || c == 'a' || c == 'b'))
        {
            return Some(clean.to_string());
        }
    }
    None
}

fn is_ccli_footer_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("for use solely with the songselect")
        || lower.contains("terms of use")
        || lower.contains("all rights reserved")
        || lower.contains("www.ccli.com")
        || lower.contains("ccli license #")
        || lower.contains("ccli license no")
        || (lower.starts_with("page ") && lower.contains(" of "))
}

fn is_chord_only_line(line: &str) -> bool {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.is_empty() {
        return false;
    }
    let mut chord_count = 0usize;
    for tok in &tokens {
        if is_chord_token(tok) {
            chord_count += 1;
            continue;
        }
        // Allow standalone measure barlines (`|`, `||`, `|:`, `:|`, `/`, `-`) and repeat markers (`2x`, `x2`).
        let bar_or_repeat = tok
            .chars()
            .all(|c| matches!(c, '|' | ':' | '/' | '-' | '(' | ')' | 'x' | 'X' | '0'..='9'));
        if !bar_or_repeat {
            return false;
        }
    }
    chord_count > 0
}

fn is_chord_token(tok: &str) -> bool {
    let clean = tok.trim_matches(|c| c == '(' || c == ')' || c == '|' || c == '-');
    if clean.is_empty() || clean.len() > 11 {
        return false;
    }
    let bytes = clean.as_bytes();
    if !(b'A'..=b'G').contains(&bytes[0]) {
        return false;
    }
    let rest = &clean[1..];
    if rest.is_empty() {
        return true;
    }
    let allowed = [
        "m", "maj", "min", "sus", "add", "dim", "aug", "2", "4", "5", "6", "7", "9", "11", "13",
        "#", "b", "/", "A", "B", "C", "D", "E", "F", "G",
    ];
    let mut rem = rest;
    while !rem.is_empty() {
        if let Some(m) = allowed.iter().find(|p| rem.starts_with(**p)) {
            rem = &rem[m.len()..];
        } else {
            return false;
        }
    }
    true
}

fn extract_ccli_number(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    if !lower.contains("ccli") || lower.contains("license") {
        return None;
    }
    let digits: String = line
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.len() >= 4 {
        Some(digits)
    } else {
        None
    }
}

/// Parses extracted lyric-sheet or chord-chart text (SongSelect / Planning Center format) into
/// structured metadata, clean vocalist lyric slides, full musician [`ChordChartDocument`] with
/// aligned chord-over-lyric lines & instrumental sections, and a default non-destructive [`Arrangement`].
pub fn parse_lyric_sheet_text(
    raw_text: &str,
    fallback_title: Option<&str>,
) -> Result<ParsedLyricSheet, PdfLyricsError> {
    let mut title = String::new();
    let mut author = String::new();
    let mut copyright = String::new();
    let mut ccli_number: Option<String> = None;
    let mut key_signature: Option<String> = None;
    let mut tempo_bpm: Option<f32> = None;
    let mut time_signature: Option<String> = None;

    let mut current_group: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();
    let mut current_chords: Vec<String> = Vec::new();
    let mut current_chart_lines: Vec<ChordChartLine> = Vec::new();
    let mut pending_chord_line: Option<String> = None;
    let mut has_any_chords = false;

    let mut sections: Vec<(String, Vec<String>, Option<String>)> = Vec::new();
    let mut chart_sections: Vec<ChordChartSection> = Vec::new();

    let flush_section = |group: &str,
                         lines: &mut Vec<String>,
                         chords: &mut Vec<String>,
                         chart_lines: &mut Vec<ChordChartLine>,
                         pending_chord: &mut Option<String>,
                         sections: &mut Vec<(String, Vec<String>, Option<String>)>,
                         chart_sections: &mut Vec<ChordChartSection>| {
        if let Some(dangling_chord) = pending_chord.take() {
            chart_lines.push(ChordChartLine {
                chords: Some(dangling_chord),
                lyrics: String::new(),
            });
        }
        if !lines.is_empty() {
            let chords_opt = if chords.is_empty() {
                None
            } else {
                Some(chords.join(" / "))
            };
            sections.push((group.to_string(), std::mem::take(lines), chords_opt));
        }
        if !chart_lines.is_empty() {
            chart_sections.push(ChordChartSection {
                group: group.to_string(),
                lines: std::mem::take(chart_lines),
            });
        }
        chords.clear();
    };

    for raw in raw_text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            if let Some(ref g) = current_group {
                flush_section(
                    g,
                    &mut current_lines,
                    &mut current_chords,
                    &mut current_chart_lines,
                    &mut pending_chord_line,
                    &mut sections,
                    &mut chart_sections,
                );
            }
            continue;
        }

        if let Some(num) = extract_ccli_number(line) {
            if ccli_number.is_none() {
                ccli_number = Some(num);
            }
            continue;
        }
        if is_ccli_footer_line(line) {
            continue;
        }
        if line.starts_with('©')
            || line.to_ascii_lowercase().starts_with("(c)")
            || line.to_ascii_lowercase().starts_with("copyright")
        {
            if copyright.is_empty() {
                copyright = line.to_string();
            }
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("words and music by")
            || lower.starts_with("words & music by")
            || lower.starts_with("by ")
        {
            if author.is_empty() {
                author = line
                    .trim_start_matches("Words and Music by")
                    .trim_start_matches("Words & Music by")
                    .trim_start_matches("By")
                    .trim_start_matches("by")
                    .trim()
                    .to_string();
            }
            continue;
        }
        if lower.starts_with("key -") || lower.starts_with("key:") {
            let k = line
                .split_once('-')
                .or_else(|| line.split_once(':'))
                .map(|(_, r)| r.trim().to_string())
                .filter(|s| !s.is_empty());
            if key_signature.is_none() {
                key_signature = k;
            }
            continue;
        }
        if lower.starts_with("tempo") || lower.starts_with("bpm") {
            let digits: String = line
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if tempo_bpm.is_none()
                && let Ok(v) = digits.parse::<f32>()
                && (30.0..=260.0).contains(&v)
            {
                tempo_bpm = Some(v);
            }
            if lower.contains("time")
                && let Some(ts) = line.split_whitespace().find(|t| t.contains('/') && t.len() <= 5)
            {
                time_signature = Some(ts.to_string());
            }
            continue;
        }
        if lower.starts_with("time -") || lower.starts_with("time:") {
            let ts = line
                .split_once('-')
                .or_else(|| line.split_once(':'))
                .map(|(_, r)| r.trim().to_string())
                .filter(|s| !s.is_empty());
            if time_signature.is_none() {
                time_signature = ts;
            }
            continue;
        }

        if let Some(sec_name) = detect_section_header(line) {
            if let Some(ref g) = current_group {
                flush_section(
                    g,
                    &mut current_lines,
                    &mut current_chords,
                    &mut current_chart_lines,
                    &mut pending_chord_line,
                    &mut sections,
                    &mut chart_sections,
                );
            }
            current_group = Some(sec_name);
            continue;
        }

        // First non-metadata line before any section header is the Song Title.
        if current_group.is_none() && title.is_empty() && !is_chord_only_line(line) {
            title = line.to_string();
            continue;
        }

        // Capture chord-only lines while preserving relative spacing for musician Chord Charts.
        if is_chord_only_line(line) {
            has_any_chords = true;
            if current_group.is_none() {
                current_group = Some("Intro".to_string());
            }
            if let Some(prev_chord) = pending_chord_line.replace(raw.trim_end().to_string()) {
                current_chart_lines.push(ChordChartLine {
                    chords: Some(prev_chord),
                    lyrics: String::new(),
                });
            }
            current_chords.push(line.to_string());
            continue;
        }

        if current_group.is_none() {
            current_group = Some("Verse 1".to_string());
        }
        current_chart_lines.push(ChordChartLine {
            chords: pending_chord_line.take(),
            lyrics: line.to_string(),
        });
        current_lines.push(line.to_string());
    }

    if let Some(ref g) = current_group {
        flush_section(
            g,
            &mut current_lines,
            &mut current_chords,
            &mut current_chart_lines,
            &mut pending_chord_line,
            &mut sections,
            &mut chart_sections,
        );
    }

    if title.is_empty() {
        title = fallback_title
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("Imported Lyric Sheet")
            .to_string();
    }

    if sections.is_empty() && chart_sections.is_empty() {
        return Err(PdfLyricsError::NoTextExtracted);
    }

    // Deduplicate master slides by group name while preserving full sequence in Default Arrangement.
    let mut slides = Vec::new();
    let mut group_order = Vec::new();
    let mut seen_groups = std::collections::HashSet::new();

    for (group, lines, chords_opt) in sections {
        group_order.push(group.clone());
        let key = group.to_ascii_lowercase();
        if !seen_groups.insert(key) {
            continue;
        }
        // Chunk long sections into readable 4-line vocalist slides.
        for (chunk_idx, chunk) in lines.chunks(4).enumerate() {
            let label = if lines.len() > 4 {
                format!("{title} — {group} ({})", chunk_idx + 1)
            } else {
                format!("{title} — {group}")
            };
            let mut slide = slide_from_text(&label, &group, &chunk.join("\n"), ccli_number.clone());
            if chunk_idx == 0 {
                slide.chords_line = chords_opt.clone();
            }
            slides.push(slide);
        }
    }

    let chord_chart = if has_any_chords || !chart_sections.is_empty() {
        Some(ChordChartDocument {
            title: title.clone(),
            key_signature: key_signature.clone(),
            tempo_bpm,
            time_signature: time_signature.clone(),
            sections: chart_sections,
        })
    } else {
        None
    };

    let arrangements = vec![Arrangement {
        id: "arr-pdf-default".to_string(),
        name: "Default".to_string(),
        group_order,
    }];

    Ok(ParsedLyricSheet {
        title,
        author,
        copyright,
        ccli_number,
        key_signature,
        tempo_bpm,
        time_signature,
        slides,
        arrangements,
        chord_chart,
        raw_text_preview: raw_text.chars().take(600).collect(),
    })
}

/// Converts a ChordPro file (`{title: ...}`, `{key: G}`, `{tempo: 76}`, `[G]Amazing [C]grace`)
/// into a full structured musician [`ChordChartDocument`] with aligned chord-over-lyric lines.
pub fn parse_chordpro_to_chart(input: &str, fallback_title: Option<&str>) -> ChordChartDocument {
    let mut title = fallback_title
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Imported Chord Chart")
        .to_string();
    let mut key_signature: Option<String> = None;
    let mut tempo_bpm: Option<f32> = None;
    let mut time_signature: Option<String> = None;

    let mut sections: Vec<ChordChartSection> = Vec::new();
    let mut current_group = "Verse 1".to_string();
    let mut current_lines: Vec<ChordChartLine> = Vec::new();

    for raw in input.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            if !current_lines.is_empty() {
                sections.push(ChordChartSection {
                    group: current_group.clone(),
                    lines: std::mem::take(&mut current_lines),
                });
            }
            continue;
        }
        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            let inner = &trimmed[1..trimmed.len() - 1];
            let lower = inner.to_ascii_lowercase();
            if lower == "soc" || lower == "start_of_chorus" {
                if !current_lines.is_empty() {
                    sections.push(ChordChartSection {
                        group: current_group.clone(),
                        lines: std::mem::take(&mut current_lines),
                    });
                }
                current_group = "Chorus".to_string();
                continue;
            }
            if lower == "sob" || lower == "start_of_bridge" {
                if !current_lines.is_empty() {
                    sections.push(ChordChartSection {
                        group: current_group.clone(),
                        lines: std::mem::take(&mut current_lines),
                    });
                }
                current_group = "Bridge".to_string();
                continue;
            }
            if let Some((k, v)) = inner.split_once(':') {
                let k = k.trim().to_ascii_lowercase();
                let v = v.trim();
                match k.as_str() {
                    "t" | "title" if !v.is_empty() => title = v.to_string(),
                    "key" if !v.is_empty() => key_signature = Some(v.to_string()),
                    "tempo" | "bpm" => tempo_bpm = v.parse::<f32>().ok(),
                    "time" if !v.is_empty() => time_signature = Some(v.to_string()),
                    "c" | "comment" if !v.is_empty() => {
                        if let Some(sec) = detect_section_header(v) {
                            if !current_lines.is_empty() {
                                sections.push(ChordChartSection {
                                    group: current_group.clone(),
                                    lines: std::mem::take(&mut current_lines),
                                });
                            }
                            current_group = sec;
                        }
                    }
                    _ => {}
                }
            }
            continue;
        }
        if let Some(sec) = detect_section_header(trimmed) {
            if !current_lines.is_empty() {
                sections.push(ChordChartSection {
                    group: current_group.clone(),
                    lines: std::mem::take(&mut current_lines),
                });
            }
            current_group = sec;
            continue;
        }

        // Convert inline `[G]Amazing [C]grace` into aligned chord row + lyric row.
        let mut chord_row = String::new();
        let mut lyric_row = String::new();
        let mut has_chord = false;
        let mut chars = trimmed.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '[' {
                let mut chord = String::new();
                for c in chars.by_ref() {
                    if c == ']' {
                        break;
                    }
                    chord.push(c);
                }
                if !chord.is_empty() {
                    has_chord = true;
                    while chord_row.len() < lyric_row.len() {
                        chord_row.push(' ');
                    }
                    if !chord_row.is_empty() && !chord_row.ends_with(' ') {
                        chord_row.push(' ');
                    }
                    chord_row.push_str(&chord);
                }
            } else {
                lyric_row.push(ch);
            }
        }
        current_lines.push(ChordChartLine {
            chords: if has_chord { Some(chord_row) } else { None },
            lyrics: lyric_row.trim_end().to_string(),
        });
    }
    if !current_lines.is_empty() {
        sections.push(ChordChartSection {
            group: current_group,
            lines: current_lines,
        });
    }

    ChordChartDocument {
        title,
        key_signature,
        tempo_bpm,
        time_signature,
        sections,
    }
}

/// Extracts embedded JPEG image streams (`/DCTDecode` or `FF D8 FF`) from a Sheet Music PDF
/// into collision-safe local files in `output_dir` so scanned/rasterized full-clef sheet music
/// pages render directly on Musician Stage Displays.
fn extract_pdf_embedded_jpegs(
    pdf_bytes: &[u8],
    title_slug: &str,
    output_dir: &Path,
) -> Vec<String> {
    let mut extracted_paths = Vec::new();
    let _ = std::fs::create_dir_all(output_dir);
    let mut pos = 0usize;
    let mut page_idx = 0usize;

    while let Some(stream_rel) = find_bytes(&pdf_bytes[pos..], b"stream") {
        let stream_kw = pos + stream_rel;
        let dict_start = stream_kw.saturating_sub(512);
        let dict_slice = &pdf_bytes[dict_start..stream_kw];
        let is_dct = find_bytes(dict_slice, b"DCTDecode").is_some();

        let mut data_start = stream_kw + b"stream".len();
        if data_start < pdf_bytes.len() && pdf_bytes[data_start] == b'\r' {
            data_start += 1;
        }
        if data_start < pdf_bytes.len() && pdf_bytes[data_start] == b'\n' {
            data_start += 1;
        } else {
            pos = stream_kw + 6;
            continue;
        }

        let Some(end_rel) = find_bytes(&pdf_bytes[data_start..], b"endstream") else {
            break;
        };
        let mut data_end = data_start + end_rel;
        while data_end > data_start
            && (pdf_bytes[data_end - 1] == b'\n' || pdf_bytes[data_end - 1] == b'\r')
        {
            data_end -= 1;
        }
        let raw = &pdf_bytes[data_start..data_end];
        if (is_dct || (raw.len() > 16 && raw[0] == 0xFF && raw[1] == 0xD8 && raw[2] == 0xFF))
            && raw.len() >= 16
        {
            page_idx += 1;
            let hash8 = &blake3::hash(raw).to_hex()[..8];
            let file_name = format!("{title_slug}__sheet_p{page_idx:02}__{hash8}.jpg");
            let dest = output_dir.join(file_name);
            if std::fs::write(&dest, raw).is_ok() {
                extracted_paths.push(dest.to_string_lossy().to_string());
            }
        }

        pos = data_start + end_rel + b"endstream".len();
    }

    extracted_paths
}

fn count_pdf_pages_approx(pdf_bytes: &[u8]) -> usize {
    let mut count = 0usize;
    let mut pos = 0usize;
    while let Some(rel) = find_bytes(&pdf_bytes[pos..], b"/Type") {
        let after = pos + rel + 5;
        let window_end = (after + 20).min(pdf_bytes.len());
        let snippet = &pdf_bytes[after..window_end];
        if let Some(p_rel) = find_bytes(snippet, b"/Page") {
            let next_byte = snippet.get(p_rel + 5).copied().unwrap_or(b' ');
            // Exclude `/Pages` (page tree node)
            if next_byte != b's' {
                count += 1;
            }
        }
        pos = after;
    }
    count.max(1)
}

/// Imports a multi-page Sheet Music PDF (Planning Center, SongSelect, PraiseCharts, hymnal scan)
/// into a [`SheetMusicDocument`] with full-page descriptors, extracted embedded page scans,
/// and automatic `group_to_page` synchronization for Musician Stage Displays.
pub fn import_sheet_music_pdf(
    pdf_bytes: &[u8],
    title: &str,
    source_path: Option<&str>,
    output_dir: &Path,
) -> Result<SheetMusicDocument, PdfLyricsError> {
    if pdf_bytes.len() < 8 || !pdf_bytes.starts_with(b"%PDF-") {
        return Err(PdfLyricsError::InvalidPdf(
            "missing %PDF- header".to_string(),
        ));
    }

    let slug: String = title
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .take(36)
        .collect();
    let clean_slug = if slug.trim_matches('_').is_empty() {
        "sheet_music".to_string()
    } else {
        slug
    };

    let embedded_images = extract_pdf_embedded_jpegs(pdf_bytes, &clean_slug, output_dir);
    let extracted_text = extract_pdf_text(pdf_bytes).ok();
    let page_count = count_pdf_pages_approx(pdf_bytes).max(embedded_images.len()).max(1);

    let mut group_to_page: BTreeMap<String, u32> = BTreeMap::new();
    let mut page_hints: Vec<Vec<String>> = vec![Vec::new(); page_count];

    if let Some(ref text) = extracted_text {
        let text_lines: Vec<String> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !is_ccli_footer_line(l))
            .map(String::from)
            .collect();
        let total = text_lines.len().max(1);
        for (idx, ln) in text_lines.iter().enumerate() {
            let p_idx = ((idx * page_count) / total).min(page_count - 1);
            if page_hints[p_idx].len() < 12 {
                page_hints[p_idx].push(ln.clone());
            }
            if let Some(sec) = detect_section_header(ln) {
                let norm = SheetMusicDocument::normalize_group_key(&sec);
                group_to_page.entry(norm).or_insert((p_idx + 1) as u32);
            }
        }
    }

    let mut pages = Vec::with_capacity(page_count);
    for (idx, hints) in page_hints.into_iter().enumerate().take(page_count) {
        let p_num = (idx + 1) as u32;
        pages.push(SheetMusicPage {
            page_number: p_num,
            label: format!("Page {p_num}"),
            image_path: embedded_images.get(idx).cloned(),
            staff_lines_hint: hints,
        });
    }

    Ok(SheetMusicDocument {
        title: title.to_string(),
        source_path: source_path.map(String::from),
        pages,
        group_to_page,
        slide_to_page: BTreeMap::new(),
    })
}

/// Builds a [`SheetMusicDocument`] directly from one or more local PNG/JPG/BMP page images
/// (e.g., an exported folder of sheet music score pages).
pub fn build_sheet_music_from_images(image_paths: &[PathBuf], title: &str) -> SheetMusicDocument {
    let mut sorted = image_paths.to_vec();
    sorted.sort();
    let pages = sorted
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let p_num = (i + 1) as u32;
            let stem = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Page")
                .to_string();
            SheetMusicPage {
                page_number: p_num,
                label: format!("Page {p_num} ({stem})"),
                image_path: Some(p.to_string_lossy().to_string()),
                staff_lines_hint: Vec::new(),
            }
        })
        .collect();
    SheetMusicDocument {
        title: title.to_string(),
        source_path: sorted.first().map(|p| p.to_string_lossy().to_string()),
        pages,
        group_to_page: BTreeMap::new(),
        slide_to_page: BTreeMap::new(),
    }
}

/// Full pipeline: reads raw SongSelect / Planning Center PDF bytes and produces editable slides,
/// full musician [`ChordChartDocument`], and CCLI metadata.
pub fn import_lyric_sheet_pdf(
    pdf_bytes: &[u8],
    fallback_title: Option<&str>,
) -> Result<ParsedLyricSheet, PdfLyricsError> {
    let text = extract_pdf_text(pdf_bytes)?;
    parse_lyric_sheet_text(&text, fallback_title)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    fn make_test_pdf(content_stream: &[u8], compress: bool) -> Vec<u8> {
        let mut pdf = Vec::new();
        pdf.extend_from_slice(b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog >>\nendobj\n");
        if compress {
            let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
            enc.write_all(content_stream).expect("zlib write");
            let zipped = enc.finish().expect("zlib finish");
            pdf.extend_from_slice(
                format!(
                    "2 0 obj\n<< /Length {} /Filter /FlateDecode >>\nstream\n",
                    zipped.len()
                )
                .as_bytes(),
            );
            pdf.extend_from_slice(&zipped);
            pdf.extend_from_slice(b"\nendstream\nendobj\n%%EOF\n");
        } else {
            pdf.extend_from_slice(
                format!("2 0 obj\n<< /Length {} >>\nstream\n", content_stream.len()).as_bytes(),
            );
            pdf.extend_from_slice(content_stream);
            pdf.extend_from_slice(b"\nendstream\nendobj\n%%EOF\n");
        }
        pdf
    }

    #[test]
    fn parses_flate_compressed_songselect_lyric_and_chord_pdf_with_ccli_and_arrangements() {
        let stream = br#"BT
/F1 18 Tf 72 720 Td (Goodness Of God) Tj
0 -18 Td (Words and Music by Ben Fielding and Jenn Johnson) Tj
0 -14 Td (Key - A) Tj
0 -20 Td (Verse 1) Tj
0 -14 Td (A      D       A) Tj
0 -14 Td [(I love ) -200 (You Lord)] TJ
0 -14 Td (Oh Your mercy never fails me) Tj
0 -20 Td (Chorus) Tj
0 -14 Td (All my life You have been faithful) Tj
0 -14 Td (All my life You have been so so good) Tj
0 -20 Td (Verse 2) Tj
0 -14 Td (I love Your voice) Tj
0 -20 Td (Chorus) Tj
0 -14 Td (All my life You have been faithful) Tj
0 -24 Td (CCLI Song # 7117726) Tj
0 -12 Td (\251 2018 Bethel Music Publishing) Tj
0 -12 Td (For use solely with the SongSelect Terms of Use. All rights reserved. www.ccli.com) Tj
0 -12 Td (CCLI License # 123456) Tj
ET"#;

        let pdf_bytes = make_test_pdf(stream, true);
        let parsed = import_lyric_sheet_pdf(&pdf_bytes, Some("Fallback")).expect("parse flate pdf");
        assert_eq!(parsed.title, "Goodness Of God");
        assert_eq!(parsed.author, "Ben Fielding and Jenn Johnson");
        assert_eq!(parsed.ccli_number.as_deref(), Some("7117726"));
        assert_eq!(parsed.key_signature.as_deref(), Some("A"));
        assert!(parsed.copyright.contains("Bethel Music"));
        // Master slides deduplicate repeated Chorus while Default arrangement preserves Verse 1 -> Chorus -> Verse 2 -> Chorus!
        assert_eq!(parsed.slides.len(), 3);
        assert_eq!(
            parsed.arrangements[0].group_order,
            vec!["Verse 1", "Chorus", "Verse 2", "Chorus"]
        );
        // Chord line stripped from lyric body and captured on chords_line
        assert_eq!(parsed.slides[0].chords_line.as_deref(), Some("A      D       A"));
        assert!(!parsed.slides[0].plain_text().contains("SongSelect"));
        assert!(!parsed.slides[0].plain_text().contains("CCLI License"));
    }

    #[test]
    fn parses_uncompressed_pdf_with_hex_and_utf16_strings_and_rejects_corrupt_inputs() {
        let stream = b"BT\n1 0 0 1 72 700 Tm <FEFF0048006F006C0079> Tj\n1 0 0 1 72 680 Tm (Verse 1) Tj\n1 0 0 1 72 660 Tm <486F6C792061726520596F75> Tj\nT* (Lord God \\(Almighty\\)) '\nET";
        let pdf = make_test_pdf(stream, false);
        let parsed = import_lyric_sheet_pdf(&pdf, None).expect("parse uncompressed");
        assert_eq!(parsed.title, "Holy");
        assert_eq!(parsed.slides.len(), 1);
        assert!(parsed.slides[0].plain_text().contains("Lord God (Almighty)"));

        // Invalid PDF and empty text error paths
        assert!(import_lyric_sheet_pdf(b"not a pdf", None).is_err());
        let empty_pdf = make_test_pdf(b"0 0 m 10 10 l S", false);
        assert!(import_lyric_sheet_pdf(&empty_pdf, None).is_err());
    }

    #[test]
    fn parses_chord_chart_metadata_chordpro_and_multipage_sheet_music_pdf() {
        let chart_text = "\
Holy Forever
Tempo - 72
Time - 4/4
Key - Bb
Intro
| Bb   Eb/Bb | Gm7   F |
Verse 1
Bb               Eb
A thousand generations falling down in worship
Gm7              F
To sing the song of ages to the Lamb
Chorus
Eb               F
Your name is the highest";
        let parsed = parse_lyric_sheet_text(chart_text, None).expect("parse chord chart");
        assert_eq!(parsed.tempo_bpm, Some(72.0));
        assert_eq!(parsed.time_signature.as_deref(), Some("4/4"));
        let chart = parsed.chord_chart.expect("chord chart populated");
        assert_eq!(chart.sections.len(), 3);
        assert_eq!(chart.sections[0].group, "Intro");
        assert!(chart.sections[0].lines[0].lyrics.is_empty());
        assert_eq!(
            chart.sections[1].lines[0].chords.as_deref(),
            Some("Bb               Eb")
        );

        // ChordPro -> aligned ChordChartDocument
        let cp = parse_chordpro_to_chart(
            "{title: Amazing Grace}\n{key: G}\n{tempo: 74}\n{time: 3/4}\n{soc}\n[G]Amazing [C]grace how [G]sweet\n{sob}\n[Em]The earth shall [D]soon dissolve",
            None,
        );
        assert_eq!(cp.title, "Amazing Grace");
        assert_eq!(cp.key_signature.as_deref(), Some("G"));
        assert_eq!(cp.tempo_bpm, Some(74.0));
        assert_eq!(cp.time_signature.as_deref(), Some("3/4"));
        assert_eq!(cp.sections.len(), 2);
        assert_eq!(cp.sections[0].group, "Chorus");
        assert!(
            cp.sections[0].lines[0]
                .chords
                .as_deref()
                .expect("chords")
                .contains('G')
        );

        // Multi-page Sheet Music PDF with embedded JPEG page + section hints
        let mut sheet_pdf = Vec::new();
        sheet_pdf.extend_from_slice(b"%PDF-1.4\n1 0 obj\n<< /Type /Pages /Count 2 >>\nendobj\n");
        sheet_pdf.extend_from_slice(b"2 0 obj\n<< /Type /Page >>\nendobj\n3 0 obj\n<< /Type /Page >>\nendobj\n");
        let fake_jpg = [0xFFu8, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0, 1, 1, 0, 0, 1, 0xFF, 0xD9];
        sheet_pdf.extend_from_slice(b"4 0 obj\n<< /Filter /DCTDecode >>\nstream\n");
        sheet_pdf.extend_from_slice(&fake_jpg);
        sheet_pdf.extend_from_slice(b"\nendstream\nendobj\n");
        sheet_pdf.extend_from_slice(b"5 0 obj\n<< /Length 64 >>\nstream\nBT (Verse 1) Tj T* (Chorus) Tj T* (Bridge) Tj ET\nendstream\nendobj\n%%EOF\n");

        let tmp = std::env::temp_dir().join(format!("op-sheet-test-{}", uuid::Uuid::new_v4()));
        let sheet = import_sheet_music_pdf(&sheet_pdf, "Holy Forever Piano", Some("holy.pdf"), &tmp)
            .expect("import sheet pdf");
        assert_eq!(sheet.pages.len(), 2);
        assert!(sheet.pages[0].image_path.is_some());
        assert_eq!(sheet.group_to_page.get("verse 1"), Some(&1));
        assert_eq!(sheet.group_to_page.get("bridge"), Some(&2));

        let img_sheet = build_sheet_music_from_images(
            &[PathBuf::from("page_02.png"), PathBuf::from("page_01.png")],
            "Hymnal Scan",
        );
        assert_eq!(img_sheet.pages.len(), 2);
        assert!(img_sheet.pages[0].label.contains("page_01"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
