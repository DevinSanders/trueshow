//! CCLI & SongSelect compliant workflow:
//! - Clean-room ChordPro (`.cho`/`.crd`), SongSelect `.txt`, and legacy `.usr` parsers
//! - Preserves inline chord lines for Stage Displays while stripping `[Chords]` from Audience lyrics
//! - One-click RFC-4180 CSV exporter for CCLI Reporting from local projection usage logs

use core_library::CcliUsageEntry;
use core_scene::Slide;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportedSong {
    pub title: String,
    pub author: String,
    pub copyright: String,
    pub ccli_number: Option<String>,
    pub key: Option<String>,
    pub slides: Vec<Slide>,
}

/// Strips inline `[G]` / `[D/F#]` chord brackets for clean Audience projection lyrics.
pub fn strip_inline_chords(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_bracket = false;
    for ch in line.chars() {
        match ch {
            '[' => in_bracket = true,
            ']' => in_bracket = false,
            _ if !in_bracket => out.push(ch),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parses a ChordPro (`.cho`/`.crd`/`.pro`) text document into structured slides + stage chords.
pub fn parse_chordpro(input: &str) -> Result<ImportedSong, String> {
    if input.trim().is_empty() {
        return Err("ChordPro input is empty".into());
    }
    let mut title = "Untitled Song".to_string();
    let mut author = String::new();
    let mut copyright = String::new();
    let mut ccli_number = None;
    let mut key = None;

    let mut current_group = "Verse".to_string();
    let mut current_clean_lines: Vec<String> = Vec::new();
    let mut current_chord_lines: Vec<String> = Vec::new();
    let mut slides: Vec<Slide> = Vec::new();

    let flush = |group: &str,
                 clean: &mut Vec<String>,
                 chords: &mut Vec<String>,
                 ccli: &Option<String>,
                 out: &mut Vec<Slide>| {
        if clean.is_empty() {
            return;
        }
        let body = clean.join("\n");
        let mut slide = core_library::slide_from_text(
            &format!("{group} ({})", out.len() + 1),
            group,
            &body,
            ccli.clone(),
        );
        let chord_joined = chords.join(" / ");
        if chord_joined.contains('[') {
            slide.chords_line = Some(chord_joined);
        }
        out.push(slide);
        clean.clear();
        chords.clear();
    };

    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.starts_with('#') {
            continue;
        }
        if line.is_empty() {
            flush(
                &current_group,
                &mut current_clean_lines,
                &mut current_chord_lines,
                &ccli_number,
                &mut slides,
            );
            continue;
        }
        if line.starts_with('{') && line.ends_with('}') {
            let inner = &line[1..line.len() - 1];
            let (dir, val) = inner
                .split_once(':')
                .map(|(d, v)| (d.trim().to_ascii_lowercase(), v.trim().to_string()))
                .unwrap_or_else(|| (inner.trim().to_ascii_lowercase(), String::new()));
            match dir.as_str() {
                "t" | "title" => title = val,
                "artist" | "author" | "composer" => author = val,
                "copyright" => copyright = val,
                "ccli" | "ccli_number" => ccli_number = Some(val),
                "key" => key = Some(val),
                "soc" | "start_of_chorus" => {
                    flush(
                        &current_group,
                        &mut current_clean_lines,
                        &mut current_chord_lines,
                        &ccli_number,
                        &mut slides,
                    );
                    current_group = "Chorus".to_string();
                }
                "eoc" | "end_of_chorus" => {
                    flush(
                        &current_group,
                        &mut current_clean_lines,
                        &mut current_chord_lines,
                        &ccli_number,
                        &mut slides,
                    );
                    current_group = "Verse".to_string();
                }
                "c" | "comment" if !val.is_empty() => {
                    flush(
                        &current_group,
                        &mut current_clean_lines,
                        &mut current_chord_lines,
                        &ccli_number,
                        &mut slides,
                    );
                    current_group = val;
                }
                _ => {}
            }
            continue;
        }
        current_chord_lines.push(line.to_string());
        current_clean_lines.push(strip_inline_chords(line));
    }

    flush(
        &current_group,
        &mut current_clean_lines,
        &mut current_chord_lines,
        &ccli_number,
        &mut slides,
    );

    Ok(ImportedSong {
        title,
        author,
        copyright,
        ccli_number,
        key,
        slides,
    })
}

/// Parses a legacy CCLI SongSelect `.usr` file (`[File]` INI key-value format with `/t` and `/n`).
pub fn parse_songselect_usr(input: &str) -> Result<ImportedSong, String> {
    if !input.contains("Title=") && !input.contains("Fields=") {
        return Err("invalid SongSelect .usr format: missing Title= or Fields=".into());
    }
    let mut title = "Untitled Song".to_string();
    let mut author = String::new();
    let mut copyright = String::new();
    let mut ccli_number = None;
    let mut field_names: Vec<String> = Vec::new();
    let mut field_bodies: Vec<String> = Vec::new();

    for line in input.lines() {
        let trimmed = line.trim();
        if let Some((k, v)) = trimmed.split_once('=') {
            match k.trim() {
                "Title" => title = v.trim().to_string(),
                "Author" => author = v.trim().replace(" | ", ", "),
                "Copyright" => copyright = v.trim().to_string(),
                "Admin" if copyright.is_empty() => copyright = v.trim().to_string(),
                "CCLI" | "SongNumber" => ccli_number = Some(v.trim().to_string()),
                "Fields" => {
                    field_names = v
                        .split("/t")
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "Words" => {
                    field_bodies = v
                        .split("/t")
                        .map(|s| s.replace("/n", "\n").trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                _ => {}
            }
        }
    }

    let mut slides = Vec::new();
    for (idx, body) in field_bodies.into_iter().enumerate() {
        let group = field_names
            .get(idx)
            .cloned()
            .unwrap_or_else(|| format!("Verse {}", idx + 1));
        slides.push(core_library::slide_from_text(
            &format!("{title} — {group}"),
            &group,
            &body,
            ccli_number.clone(),
        ));
    }

    Ok(ImportedSong {
        title,
        author,
        copyright,
        ccli_number,
        key: None,
        slides,
    })
}

/// Parses a SongSelect plain-text lyric download (`.txt` with trailing `CCLI Song # ...` footer).
pub fn parse_songselect_text(input: &str) -> Result<ImportedSong, String> {
    let lines: Vec<&str> = input.lines().collect();
    let first_non_empty = lines
        .iter()
        .position(|l| !l.trim().is_empty())
        .ok_or_else(|| "SongSelect text input is empty".to_string())?;
    let title = lines[first_non_empty].trim().to_string();

    let mut ccli_number = None;
    let mut author = String::new();
    let mut copyright = String::new();
    let mut body_lines = Vec::new();

    for line in &lines[first_non_empty + 1..] {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("CCLI Song #") {
            ccli_number = rest.split_whitespace().next().map(String::from);
        } else if t.starts_with('©') || t.starts_with("(c)") || t.starts_with("Copyright") {
            copyright = t.to_string();
        } else if t.starts_with("CCLI License") || t.starts_with("For use solely") {
            continue;
        } else if author.is_empty() && ccli_number.is_some() && !t.is_empty() {
            author = t.to_string();
        } else {
            body_lines.push(*line);
        }
    }

    let mut slides = Vec::new();
    let mut current_group = "Verse 1".to_string();
    let mut stanza = Vec::new();
    for raw in body_lines {
        let t = raw.trim();
        if t.is_empty() {
            if !stanza.is_empty() {
                slides.push(core_library::slide_from_text(
                    &format!("{title} — {current_group}"),
                    &current_group,
                    &stanza.join("\n"),
                    ccli_number.clone(),
                ));
                stanza.clear();
            }
            continue;
        }
        if stanza.is_empty()
            && (t.starts_with("Verse")
                || t.starts_with("Chorus")
                || t.starts_with("Bridge")
                || t.starts_with("Pre-Chorus"))
        {
            current_group = t.to_string();
        } else {
            stanza.push(t.to_string());
        }
    }
    if !stanza.is_empty() {
        slides.push(core_library::slide_from_text(
            &format!("{title} — {current_group}"),
            &current_group,
            &stanza.join("\n"),
            ccli_number.clone(),
        ));
    }

    Ok(ImportedSong {
        title,
        author,
        copyright,
        ccli_number,
        key: None,
        slides,
    })
}

fn csv_escape(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// Exports local Audience projection usage records to an RFC-4180 CSV ready for CCLI Reporting.
pub fn export_ccli_usage_csv(entries: &[CcliUsageEntry]) -> String {
    let mut out =
        String::from("ProjectedAt,ServiceProfile,SongTitle,CCLINumber,Author,DisplaySeconds\n");
    for e in entries {
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            csv_escape(&e.projected_at),
            csv_escape(&e.service_profile),
            csv_escape(&e.song_title),
            csv_escape(&e.ccli_number),
            csv_escape(&e.author),
            e.display_seconds
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chordpro_songselect_usr_text_and_exports_ccli_csv() {
        let cho = r#"
# Comment line
{title: Amazing Grace}
{author: John Newton}
{ccli: 22025}
{key: G}
{soc}
[G]Amazing [C]grace! How [G]sweet the sound
That saved a [D7]wretch like me
{eoc}
"#;
        let parsed = parse_chordpro(cho).expect("chordpro");
        assert_eq!(parsed.title, "Amazing Grace");
        assert_eq!(parsed.ccli_number.as_deref(), Some("22025"));
        assert_eq!(parsed.key.as_deref(), Some("G"));
        assert_eq!(parsed.slides.len(), 1);
        assert!(
            parsed.slides[0]
                .plain_text()
                .contains("Amazing grace! How sweet the sound")
        );
        assert!(
            parsed.slides[0]
                .chords_line
                .as_deref()
                .unwrap_or("")
                .contains("[G]")
        );
        assert!(parse_chordpro("   ").is_err());

        let usr = "[File]\nTitle=Holy Holy Holy\nAuthor=Reginald Heber\nCCLI=1156\nFields=Verse 1/tChorus\nWords=Holy, holy, holy/nLord God Almighty/tMerciful and mighty";
        let from_usr = parse_songselect_usr(usr).expect("usr");
        assert_eq!(from_usr.title, "Holy Holy Holy");
        assert_eq!(from_usr.slides.len(), 2);
        assert_eq!(from_usr.ccli_number.as_deref(), Some("1156"));
        assert!(parse_songselect_usr("not a usr file").is_err());

        let txt = "Doxology\n\nVerse 1\nPraise God from whom all blessings flow\n\nCCLI Song # 56204\nThomas Ken\n© Public Domain\n";
        let from_txt = parse_songselect_text(txt).expect("txt");
        assert_eq!(from_txt.title, "Doxology");
        assert_eq!(from_txt.ccli_number.as_deref(), Some("56204"));
        assert_eq!(from_txt.slides.len(), 1);
        assert!(parse_songselect_text("   ").is_err());

        let csv = export_ccli_usage_csv(&[CcliUsageEntry {
            id: 1,
            projected_at: "2026-11-29T10:15:00Z".into(),
            service_profile: "Sunday, 10am".into(),
            song_title: "Amazing \"Grace\"".into(),
            ccli_number: "22025".into(),
            author: "John Newton".into(),
            display_seconds: 185,
        }]);
        assert!(csv.contains("\"Sunday, 10am\""));
        assert!(csv.contains("\"Amazing \"\"Grace\"\"\""));
    }
}
