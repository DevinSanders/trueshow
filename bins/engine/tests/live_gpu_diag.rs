use core_ipc::{
    EngineCommand, EngineEvent, ShmPreviewReader, read_frame, to_local_socket_name, write_frame,
};
use core_scene::{SceneDiff, TransitionSpec};
use interprocess::local_socket::tokio::{Stream, prelude::*};
use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::process::{Command, Stdio};
use std::time::Duration;

fn checksum(bytes: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

#[tokio::test]
#[ignore = "requires an active desktop GPU display session"]
async fn live_gpu_engine_renders_both_audience_and_stage_across_slide_switches() {
    let bin = env!("CARGO_BIN_EXE_openpresenter-engine");
    let socket_id = format!("openpresenter-live-gpu-{}.sock", std::process::id());
    let preview_dir =
        std::env::temp_dir().join(format!("openpresenter-live-gpu-shm-{}", std::process::id()));

    let mut child = Command::new(bin)
        .env("RUST_BACKTRACE", "1")
        .env("RUST_LOG", "info,wgpu=warn")
        .arg("--socket")
        .arg(&socket_id)
        .arg("--preview-dir")
        .arg(&preview_dir)
        .arg("--outputs")
        .arg("2")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn live openpresenter-engine");

    let mut client = None;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Ok(name) = to_local_socket_name(&socket_id)
            && let Ok(stream) = Stream::connect(name).await
        {
            client = Some(stream);
            break;
        }
    }
    let mut client = client.expect("connect to live engine");

    // 1. Immediate GetSnapshot must wait until GPU + SHM preview rings are ready (zero race condition).
    write_frame(&mut client, &EngineCommand::GetSnapshot)
        .await
        .expect("send GetSnapshot");
    let snap = match read_frame::<_, EngineEvent>(&mut client)
        .await
        .expect("read snapshot")
    {
        EngineEvent::Snapshot(s) => *s,
        other => panic!("unexpected snapshot response: {other:?}"),
    };
    assert_ne!(
        snap.gpu_adapter_name, "Initializing...",
        "GetSnapshot returned before GPU initialization completed"
    );
    assert_eq!(
        snap.preview_buses.len(),
        2,
        "expected preview buses for both Audience and Stage immediately on connect"
    );

    let mut readers: Vec<(String, ShmPreviewReader)> = snap
        .preview_buses
        .iter()
        .map(|b| {
            (
                b.output_id.0.to_string(),
                ShmPreviewReader::open(&b.shm_path).expect("open SHM preview ring"),
            )
        })
        .collect();

    let mut buf = Vec::new();
    let mut audience_hashes = HashSet::new();
    let mut stage_hashes = HashSet::new();

    // 2. Trigger every slide in the deck and confirm BOTH Audience and Stage advance and change pixels.
    for (idx, slide) in snap.state.deck.iter().enumerate() {
        write_frame(
            &mut client,
            &EngineCommand::ApplyDiff(Box::new(SceneDiff::TriggerSlide {
                slide_id: slide.id,
                transition_override: Some(TransitionSpec::Cut),
            })),
        )
        .await
        .expect("send TriggerSlide");
        let resp = read_frame::<_, EngineEvent>(&mut client)
            .await
            .expect("read StateUpdated");
        match resp {
            EngineEvent::StateUpdated(st) => {
                assert_eq!(st.active_slide.as_ref().map(|s| s.id), Some(slide.id));
            }
            other => panic!("unexpected trigger response: {other:?}"),
        }

        tokio::time::sleep(Duration::from_millis(250)).await;
        assert!(
            child.try_wait().expect("poll engine").is_none(),
            "engine crashed after triggering slide {idx}"
        );

        let aud_meta = readers[0]
            .1
            .try_read_latest(&mut buf)
            .expect("Audience output (bus 0) did not publish a preview frame");
        let aud_hash = checksum(&buf);
        audience_hashes.insert(aud_hash);

        let stg_meta = readers[1]
            .1
            .try_read_latest(&mut buf)
            .expect("Stage output (bus 1) did not publish a preview frame");
        let stg_hash = checksum(&buf);
        stage_hashes.insert(stg_hash);

        println!(
            "[LIVE GPU] Slide {idx} ({}) -> Audience seq={} hash={aud_hash:#x} | Stage seq={} hash={stg_hash:#x}",
            slide.label, aud_meta.sequence, stg_meta.sequence,
        );
    }

    assert_eq!(
        audience_hashes.len(),
        snap.state.deck.len(),
        "expected every slide switch to produce distinct rendered pixels on Audience output"
    );
    assert_eq!(
        stage_hashes.len(),
        snap.state.deck.len(),
        "expected every slide switch to produce distinct rendered pixels on Stage output"
    );

    // 3. Verify telemetry reports real FPS and presented frames on BOTH outputs.
    write_frame(&mut client, &EngineCommand::GetTelemetry)
        .await
        .expect("send GetTelemetry");
    let tel = match read_frame::<_, EngineEvent>(&mut client)
        .await
        .expect("read telemetry")
    {
        EngineEvent::Telemetry(t) => t,
        other => panic!("unexpected telemetry: {other:?}"),
    };
    println!("[LIVE GPU] Telemetry: {tel:?}");
    assert!(tel.total_frames > 0, "expected total_frames > 0");
    assert!(tel.fps > 5.0, "expected fps > 5.0, got {}", tel.fps);
    assert_eq!(tel.outputs.len(), 2, "expected 2 outputs in telemetry");
    assert!(
        tel.outputs[0].frames_presented > 0,
        "Audience output presented 0 frames!"
    );
    assert!(
        tel.outputs[1].frames_presented > 0,
        "Stage output presented 0 frames!"
    );

    // 4. Clean shutdown.
    write_frame(&mut client, &EngineCommand::Shutdown)
        .await
        .expect("send Shutdown");
    let _ = read_frame::<_, EngineEvent>(&mut client).await;
    let status = child.wait().expect("wait engine exit");
    assert!(status.success(), "engine should exit 0 on Shutdown");
    let _ = std::fs::remove_dir_all(&preview_dir);
}
