//! Bitfocus Companion Satellite protocol parser & Stream Deck feedback state.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompanionCommand {
    Ping(String),
    AddDevice { device_id: String, product_name: String },
    KeyPress { device_id: String, key: u16, pressed: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionFeedback {
    pub active_slide_label: String,
    pub blackout: bool,
    pub timer_readout: String,
}

/// Parses a single line from the Bitfocus Companion Satellite TCP/WS protocol.
pub fn parse_satellite_line(line: &str) -> Result<CompanionCommand, String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err("empty satellite command".into());
    }
    let mut parts = trimmed.split_whitespace();
    let verb = parts.next().unwrap_or_default();

    match verb {
        "PING" => Ok(CompanionCommand::Ping(
            parts.next().unwrap_or("ok").to_string(),
        )),
        "ADD-DEVICE" => {
            let mut device_id = None;
            let mut product_name = "StreamDeck".to_string();
            for tok in parts {
                if let Some(v) = tok.strip_prefix("DEVICEID=") {
                    device_id = Some(v.trim_matches('"').to_string());
                } else if let Some(v) = tok.strip_prefix("PRODUCT_NAME=") {
                    product_name = v.trim_matches('"').to_string();
                }
            }
            let device_id = device_id.ok_or_else(|| "ADD-DEVICE missing DEVICEID".to_string())?;
            Ok(CompanionCommand::AddDevice {
                device_id,
                product_name,
            })
        }
        "KEY-PRESS" => {
            let mut device_id = None;
            let mut key = None;
            let mut pressed = false;
            for tok in parts {
                if let Some(v) = tok.strip_prefix("DEVICEID=") {
                    device_id = Some(v.trim_matches('"').to_string());
                } else if let Some(v) = tok.strip_prefix("KEY=") {
                    key = v.parse::<u16>().ok();
                } else if let Some(v) = tok.strip_prefix("PRESSED=") {
                    pressed = v.eq_ignore_ascii_case("true") || v == "1";
                }
            }
            Ok(CompanionCommand::KeyPress {
                device_id: device_id.ok_or_else(|| "KEY-PRESS missing DEVICEID".to_string())?,
                key: key.ok_or_else(|| "KEY-PRESS missing numeric KEY".to_string())?,
                pressed,
            })
        }
        other => Err(format!("unsupported Companion Satellite verb: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_companion_satellite_commands_and_rejects_invalid_lines() {
        assert_eq!(
            parse_satellite_line("PING 12345").expect("ping"),
            CompanionCommand::Ping("12345".into())
        );
        assert_eq!(
            parse_satellite_line(r#"ADD-DEVICE DEVICEID="sd-01" PRODUCT_NAME="StreamDeckXL""#)
                .expect("add"),
            CompanionCommand::AddDevice {
                device_id: "sd-01".into(),
                product_name: "StreamDeckXL".into(),
            }
        );
        assert_eq!(
            parse_satellite_line(r#"KEY-PRESS DEVICEID="sd-01" KEY=4 PRESSED=true"#).expect("key"),
            CompanionCommand::KeyPress {
                device_id: "sd-01".into(),
                key: 4,
                pressed: true,
            }
        );

        // Failure paths
        assert!(parse_satellite_line("   ").is_err());
        assert!(parse_satellite_line("UNKNOWN-VERB").is_err());
        assert!(parse_satellite_line("ADD-DEVICE PRODUCT_NAME=foo").is_err());
        assert!(parse_satellite_line("KEY-PRESS DEVICEID=sd-01 KEY=abc").is_err());
    }
}
