//! End-to-end M0 resilience test: verifies `openpresenter-engine` survives abrupt UI disconnects
//! over OS local sockets (Windows Named Pipes / Unix Domain Sockets) and preserves live state
//! across client re-attachments.

use core_ipc::{EngineCommand, EngineEvent, read_frame, to_local_socket_name, write_frame};
use core_scene::{LayerKind, SceneDiff};
use interprocess::local_socket::tokio::{Stream, prelude::*};
use std::process::{Command, Stdio};
use std::time::Duration;

#[tokio::test]
async fn engine_survives_abrupt_client_disconnect_and_preserves_scene_state() {
    let bin = env!("CARGO_BIN_EXE_openpresenter-engine");
    let socket_id = format!("openpresenter-test-resilience-{}.sock", std::process::id());

    let mut child = Command::new(bin)
        .arg("--headless")
        .arg("--socket")
        .arg(&socket_id)
        .arg("--outputs")
        .arg("2")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn openpresenter-engine --headless");

    // Wait for engine IPC listener to bind.
    let mut client1 = None;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Ok(name) = to_local_socket_name(&socket_id)
            && let Ok(stream) = Stream::connect(name).await
        {
            client1 = Some(stream);
            break;
        }
    }
    let mut client1 = client1.expect("connect client 1 to engine IPC socket");

    // Client 1 mutates live scene state.
    write_frame(
        &mut client1,
        &EngineCommand::ApplyDiff(Box::new(SceneDiff::ClearLayer(LayerKind::MediaBackground))),
    )
    .await
    .expect("send clear layer");
    let evt1: EngineEvent = read_frame(&mut client1).await.expect("read update");
    match evt1 {
        EngineEvent::StateUpdated(state) => assert!(!state.media_layer_active),
        other => panic!("expected StateUpdated, got {other:?}"),
    }

    write_frame(
        &mut client1,
        &EngineCommand::ApplyDiff(Box::new(SceneDiff::SetBlackout(true))),
    )
    .await
    .expect("send blackout");
    let _: EngineEvent = read_frame(&mut client1).await.expect("read blackout ack");

    // Simulate an abrupt Tauri UI crash by dropping client 1 without shutting down the engine.
    drop(client1);
    tokio::time::sleep(Duration::from_millis(75)).await;

    // Engine process must still be alive.
    assert!(
        child.try_wait().expect("poll engine child").is_none(),
        "engine must remain alive when UI client disconnects abruptly"
    );

    // Client 2 reconnects and receives the exact persisted state.
    let name2 = to_local_socket_name(&socket_id).expect("socket name 2");
    let mut client2 = Stream::connect(name2)
        .await
        .expect("reconnect client 2 to running engine");

    write_frame(&mut client2, &EngineCommand::GetSnapshot)
        .await
        .expect("request snapshot");
    let evt2: EngineEvent = read_frame(&mut client2).await.expect("read snapshot");
    match evt2 {
        EngineEvent::Snapshot(snap) => {
            assert!(snap.state.blackout);
            assert!(!snap.state.media_layer_active);
            assert_eq!(snap.state.outputs.len(), 2);
        }
        other => panic!("expected Snapshot, got {other:?}"),
    }

    // Explicit shutdown terminates the engine cleanly.
    write_frame(&mut client2, &EngineCommand::Shutdown)
        .await
        .expect("send shutdown");
    let evt_bye: EngineEvent = read_frame(&mut client2).await.expect("read shutdown ack");
    assert_eq!(evt_bye, EngineEvent::ShuttingDown);

    let status = child.wait().expect("wait for engine exit");
    assert!(status.success(), "engine should exit 0 on explicit Shutdown");
}
