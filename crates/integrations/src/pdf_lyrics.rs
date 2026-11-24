//! Clean-room Lyric Sheet & Chord Chart PDF parser (SongSelect / Planning Center PDFs).
//!
//! Extracts text from standard uncompressed and `/FlateDecode` (zlib) PDF content streams
//! without external binaries, parses worship section headers, strips CCLI license footers
//! and optional chord-chart lines, and builds editable [`Slide`]s + [`Arrangement`]s.

use core_library::slide_from_text;
use core_scene::{Arrangement, Slide};
use flate2::read::{DeflateDecoder, ZlibDecoder};
use serde::{Deserialize, Serialize};
use std::io::Read;
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
    pub slides: Vec<Slide>,
    pub arrangements: Vec<Arrangement>,
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
    let chord_like = tokens.iter().filter(|tok| is_chord_token(tok)).count();
    chord_like == tokens.len()
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

/// Parses extracted lyric-sheet text (SongSelect / Planning Center format) into structured
/// metadata, clean lyric slides, optional captured chords, and a default non-destructive [`Arrangement`].
pub fn parse_lyric_sheet_text(
    raw_text: &str,
    fallback_title: Option<&str>,
) -> Result<ParsedLyricSheet, PdfLyricsError> {
    let mut title = String::new();
    let mut author = String::new();
    let mut copyright = String::new();
    let mut ccli_number: Option<String> = None;
    let mut key_signature: Option<String> = None;

    let mut current_group: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();
    let mut current_chords: Vec<String> = Vec::new();
    let mut sections: Vec<(String, Vec<String>, Option<String>)> = Vec::new();

    for raw in raw_text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            if let Some(ref g) = current_group
                && !current_lines.is_empty()
            {
                let chords_opt = if current_chords.is_empty() {
                    None
                } else {
                    Some(current_chords.join(" / "))
                };
                sections.push((g.clone(), std::mem::take(&mut current_lines), chords_opt));
                current_chords.clear();
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
        if line.starts_with('©') || line.to_ascii_lowercase().starts_with("(c)") || line.to_ascii_lowercase().starts_with("copyright") {
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

        if let Some(sec_name) = detect_section_header(line) {
            if let Some(ref g) = current_group
                && !current_lines.is_empty()
            {
                let chords_opt = if current_chords.is_empty() {
                    None
                } else {
                    Some(current_chords.join(" / "))
                };
                sections.push((g.clone(), std::mem::take(&mut current_lines), chords_opt));
                current_chords.clear();
            }
            current_group = Some(sec_name);
            continue;
        }

        // First non-metadata line before any section header is the Song Title.
        if current_group.is_none() && title.is_empty() && !is_chord_only_line(line) {
            title = line.to_string();
            continue;
        }

        // Strip chord-only lines from lyric body while preserving them for stage confidence display.
        if is_chord_only_line(line) {
            current_chords.push(line.to_string());
            continue;
        }

        if current_group.is_none() {
            current_group = Some("Verse 1".to_string());
        }
        current_lines.push(line.to_string());
    }

    if let Some(g) = current_group
        && !current_lines.is_empty()
    {
        let chords_opt = if current_chords.is_empty() {
            None
        } else {
            Some(current_chords.join(" / "))
        };
        sections.push((g, current_lines, chords_opt));
    }

    if title.is_empty() {
        title = fallback_title
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("Imported Lyric Sheet")
            .to_string();
    }

    if sections.is_empty() {
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
        // Chunk long sections into readable 4-line slides.
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
        slides,
        arrangements,
        raw_text_preview: raw_text.chars().take(600).collect(),
    })
}

/// Full pipeline: reads raw SongSelect / Planning Center PDF bytes and produces editable slides + CCLI metadata.
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
}
