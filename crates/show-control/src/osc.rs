//! Bidirectional Open Sound Control (OSC) encoder/decoder built on `rosc`.

use core_scene::{LayerKind, SceneDiff, SlideId};
use rosc::{OscMessage, OscPacket, OscType};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OscError {
    #[error("malformed OSC packet: {0}")]
    MalformedPacket(String),
    #[error("unknown OSC address: {0}")]
    UnknownAddress(String),
    #[error("invalid OSC argument: {0}")]
    InvalidArgument(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum OscControlEvent {
    Scene(Box<SceneDiff>),
    RunMacro(Uuid),
}

/// Decodes raw UDP OSC bytes into OpenPresenter show-control events.
pub fn decode_osc_packet(bytes: &[u8]) -> Result<Vec<OscControlEvent>, OscError> {
    let (_, packet) = rosc::decoder::decode_udp(bytes)
        .map_err(|e| OscError::MalformedPacket(e.to_string()))?;
    let mut events = Vec::new();
    collect_packet(packet, &mut events)?;
    Ok(events)
}

fn collect_packet(packet: OscPacket, out: &mut Vec<OscControlEvent>) -> Result<(), OscError> {
    match packet {
        OscPacket::Message(msg) => out.push(route_message(msg)?),
        OscPacket::Bundle(bundle) => {
            for item in bundle.content {
                collect_packet(item, out)?;
            }
        }
    }
    Ok(())
}

fn route_message(msg: OscMessage) -> Result<OscControlEvent, OscError> {
    match msg.addr.as_str() {
        "/openpresenter/slide/trigger" => {
            let id_str = match msg.args.first() {
                Some(OscType::String(s)) => s,
                _ => return Err(OscError::InvalidArgument("expected slide UUID string".into())),
            };
            let uuid = Uuid::parse_str(id_str)
                .map_err(|e| OscError::InvalidArgument(format!("bad slide UUID: {e}")))?;
            Ok(OscControlEvent::Scene(Box::new(SceneDiff::TriggerSlide {
                slide_id: SlideId(uuid),
                transition_override: None,
            })))
        }
        "/openpresenter/blackout" => {
            let on = match msg.args.first() {
                Some(OscType::Bool(b)) => *b,
                Some(OscType::Int(i)) => *i != 0,
                _ => return Err(OscError::InvalidArgument("expected bool or int".into())),
            };
            Ok(OscControlEvent::Scene(Box::new(SceneDiff::SetBlackout(on))))
        }
        "/openpresenter/layer/clear" => {
            let layer_name = match msg.args.first() {
                Some(OscType::String(s)) => s.as_str(),
                _ => return Err(OscError::InvalidArgument("expected layer name string".into())),
            };
            let layer = match layer_name.to_ascii_lowercase().as_str() {
                "video" => LayerKind::VideoInput,
                "media" => LayerKind::MediaBackground,
                "slide" => LayerKind::Slide,
                "announcements" => LayerKind::Announcements,
                "props" | "overlays" => LayerKind::PropsAndOverlays,
                "masks" => LayerKind::Masks,
                "messages" | "timers" => LayerKind::MessagesAndTimers,
                other => return Err(OscError::InvalidArgument(format!("unknown layer '{other}'"))),
            };
            Ok(OscControlEvent::Scene(Box::new(SceneDiff::ClearLayer(
                layer,
            ))))
        }
        "/openpresenter/stage/message" => {
            let text = match msg.args.first() {
                Some(OscType::String(s)) if !s.trim().is_empty() => Some(s.clone()),
                _ => None,
            };
            Ok(OscControlEvent::Scene(Box::new(SceneDiff::SetStageMessage(
                text,
            ))))
        }
        "/openpresenter/macro/run" => {
            let id_str = match msg.args.first() {
                Some(OscType::String(s)) => s,
                _ => return Err(OscError::InvalidArgument("expected macro UUID string".into())),
            };
            let uuid = Uuid::parse_str(id_str)
                .map_err(|e| OscError::InvalidArgument(format!("bad macro UUID: {e}")))?;
            Ok(OscControlEvent::RunMacro(uuid))
        }
        other => Err(OscError::UnknownAddress(other.to_string())),
    }
}

/// Encodes an outbound OSC message (e.g., triggering lighting cues or video switchers).
pub fn encode_osc_event(address: &str, args: Vec<OscType>) -> Result<Vec<u8>, OscError> {
    if !address.starts_with('/') {
        return Err(OscError::InvalidArgument(
            "OSC address must start with '/'".into(),
        ));
    }
    rosc::encoder::encode(&OscPacket::Message(OscMessage {
        addr: address.to_string(),
        args,
    }))
    .map_err(|e| OscError::MalformedPacket(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_valid_osc_commands_and_rejects_malformed_or_unknown_packets() {
        let slide_uuid = Uuid::from_u128(1002);
        let pkt_slide = encode_osc_event(
            "/openpresenter/slide/trigger",
            vec![OscType::String(slide_uuid.to_string())],
        )
        .expect("encode slide");
        assert_eq!(
            decode_osc_packet(&pkt_slide).expect("decode slide"),
            vec![OscControlEvent::Scene(Box::new(SceneDiff::TriggerSlide {
                slide_id: SlideId(slide_uuid),
                transition_override: None,
            }))]
        );

        let pkt_bo = encode_osc_event("/openpresenter/blackout", vec![OscType::Int(1)]).expect("bo");
        assert_eq!(
            decode_osc_packet(&pkt_bo).expect("decode bo"),
            vec![OscControlEvent::Scene(Box::new(SceneDiff::SetBlackout(
                true
            )))]
        );

        let pkt_clr = encode_osc_event(
            "/openpresenter/layer/clear",
            vec![OscType::String("media".into())],
        )
        .expect("clr");
        assert_eq!(
            decode_osc_packet(&pkt_clr).expect("decode clr"),
            vec![OscControlEvent::Scene(Box::new(SceneDiff::ClearLayer(
                LayerKind::MediaBackground
            )))]
        );

        let pkt_msg = encode_osc_event(
            "/openpresenter/stage/message",
            vec![OscType::String("Wrap up".into())],
        )
        .expect("msg");
        assert_eq!(
            decode_osc_packet(&pkt_msg).expect("decode msg"),
            vec![OscControlEvent::Scene(Box::new(SceneDiff::SetStageMessage(
                Some("Wrap up".into())
            )))]
        );

        let macro_uuid = Uuid::from_u128(77);
        let pkt_mac = encode_osc_event(
            "/openpresenter/macro/run",
            vec![OscType::String(macro_uuid.to_string())],
        )
        .expect("mac");
        assert_eq!(
            decode_osc_packet(&pkt_mac).expect("decode mac"),
            vec![OscControlEvent::RunMacro(macro_uuid)]
        );

        // Failure paths
        assert!(matches!(
            encode_osc_event("no-leading-slash", vec![]),
            Err(OscError::InvalidArgument(_))
        ));
        assert!(matches!(
            decode_osc_packet(&[0x01, 0x02]),
            Err(OscError::MalformedPacket(_))
        ));
        let unknown = encode_osc_event("/unknown/path", vec![]).expect("unk");
        assert!(matches!(
            decode_osc_packet(&unknown),
            Err(OscError::UnknownAddress(_))
        ));
        let bad_layer = encode_osc_event(
            "/openpresenter/layer/clear",
            vec![OscType::String("invalid_layer".into())],
        )
        .expect("bad layer");
        assert!(matches!(
            decode_osc_packet(&bad_layer),
            Err(OscError::InvalidArgument(_))
        ));
    }
}
