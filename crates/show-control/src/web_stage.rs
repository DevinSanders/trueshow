//! Embedded `axum` HTTP + WebSocket LAN Browser Stage Display server.
//!
//! Lets worship leaders and musicians view live Current Slide, Next Slide, Chords, Timer,
//! and urgent Stage Messages from any tablet/phone on sanctuary Wi-Fi without touching
//! the native `wgpu` Audience or Stage outputs.

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::{Html, IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use core_scene::SceneState;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tokio::net::TcpListener;
use tokio::sync::broadcast;

/// Live JSON payload pushed to connected LAN browser Stage Display clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageViewPayload {
    pub revision: u64,
    pub blackout: bool,
    pub current_label: String,
    pub current_text: String,
    pub chords_line: Option<String>,
    pub next_text: Option<String>,
    pub stage_message: Option<String>,
    pub timer_readout: Option<String>,
}

/// Builds a [`StageViewPayload`] snapshot directly from canonical [`SceneState`].
pub fn payload_from_scene(state: &SceneState) -> StageViewPayload {
    let (current_label, current_text, chords_line) = match &state.active_slide {
        Some(slide) if state.slide_layer_active && !state.blackout => (
            slide.label.clone(),
            slide.plain_text(),
            slide.chords_line.clone(),
        ),
        _ => (String::new(), String::new(), None),
    };

    let next_text = state
        .next_slide_preview
        .as_ref()
        .map(|s| s.plain_text().replace('\n', " / "));

    StageViewPayload {
        revision: state.revision,
        blackout: state.blackout,
        current_label,
        current_text,
        chords_line,
        next_text,
        stage_message: state.stage_message.clone(),
        timer_readout: state.timer_readout.clone(),
    }
}

/// Publish/subscribe state hub backing both HTTP polling and live WebSocket Stage clients.
#[derive(Clone)]
pub struct StageHub {
    latest: Arc<RwLock<StageViewPayload>>,
    tx: broadcast::Sender<StageViewPayload>,
}

impl StageHub {
    pub fn new(initial: StageViewPayload) -> Self {
        let (tx, _) = broadcast::channel(32);
        Self {
            latest: Arc::new(RwLock::new(initial)),
            tx,
        }
    }

    pub fn latest(&self) -> StageViewPayload {
        self.latest
            .read()
            .map(|g| g.clone())
            .unwrap_or(StageViewPayload {
                revision: 0,
                blackout: true,
                current_label: String::new(),
                current_text: String::new(),
                chords_line: None,
                next_text: None,
                stage_message: None,
                timer_readout: None,
            })
    }

    pub fn publish(&self, payload: StageViewPayload) {
        if let Ok(mut guard) = self.latest.write() {
            *guard = payload.clone();
        }
        let _ = self.tx.send(payload);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<StageViewPayload> {
        self.tx.subscribe()
    }
}

/// Handle to a running LAN Browser Stage Display server.
pub struct WebStageServer {
    addr: SocketAddr,
    hub: StageHub,
}

impl WebStageServer {
    pub async fn bind(bind_addr: &str, initial: StageViewPayload) -> Result<Self, std::io::Error> {
        let listener = TcpListener::bind(bind_addr).await?;
        let addr = listener.local_addr()?;
        let hub = StageHub::new(initial);
        let router = build_stage_router_with_hub(hub.clone());
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok(Self { addr, hub })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Publishes an updated stage snapshot to HTTP readers and all active WebSocket clients.
    pub fn publish(&self, payload: StageViewPayload) {
        self.hub.publish(payload);
    }
}

/// Builds the standalone Axum router (useful for unit testing and custom listeners).
pub fn build_stage_router(initial: StageViewPayload) -> Router {
    build_stage_router_with_hub(StageHub::new(initial))
}

pub fn build_stage_router_with_hub(hub: StageHub) -> Router {
    Router::new()
        .route("/", get(stage_html))
        .route("/api/stage/state", get(stage_json))
        .route("/ws/stage", get(stage_ws))
        .with_state(hub)
}

async fn stage_html() -> Html<&'static str> {
    Html(STAGE_DISPLAY_HTML)
}

async fn stage_json(State(hub): State<StageHub>) -> Json<StageViewPayload> {
    Json(hub.latest())
}

async fn stage_ws(ws: WebSocketUpgrade, State(hub): State<StageHub>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_client(socket, hub))
}

async fn handle_ws_client(mut socket: WebSocket, hub: StageHub) {
    let initial = hub.latest();
    if let Ok(json) = serde_json::to_string(&initial)
        && socket.send(Message::Text(json.into())).await.is_err()
    {
        return;
    }

    let mut rx = hub.subscribe();
    while let Ok(update) = rx.recv().await {
        if let Ok(json) = serde_json::to_string(&update)
            && socket.send(Message::Text(json.into())).await.is_err()
        {
            break;
        }
    }
}

const STAGE_DISPLAY_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width,initial-scale=1" />
<title>OpenPresenter — LAN Stage Display</title>
<style>
  body { margin:0; background:#07090f; color:#f4f7fc; font-family:system-ui,sans-serif; display:flex; flex-direction:column; height:100vh; }
  header { display:flex; justify-content:space-between; padding:14px 24px; background:#101521; border-bottom:2px solid #2563eb; font-size:20px; }
  #timer { color:#34d399; font-weight:700; font-variant-numeric:tabular-nums; }
  main { flex:1; display:flex; flex-direction:column; justify-content:center; align-items:center; padding:24px; text-align:center; gap:18px; }
  #chords { color:#fbbf24; font-family:monospace; font-size:28px; }
  #curr { font-size:52px; font-weight:700; white-space:pre-line; line-height:1.25; }
  footer { padding:18px 24px; background:#0e131e; border-top:1px solid #1e293b; font-size:24px; color:#94a3b8; }
  #alert { display:none; padding:14px 24px; background:#dc2626; color:#fff; font-size:28px; font-weight:700; text-align:center; }
</style>
</head>
<body>
  <header><span id="label">OpenPresenter Stage</span><span id="timer">--:--</span></header>
  <main><div id="chords"></div><div id="curr">Waiting for cue...</div></main>
  <div id="alert"></div>
  <footer>Next: <span id="next">—</span></footer>
  <script>
    function apply(d) {
      document.getElementById('label').textContent = d.current_label || 'OpenPresenter Stage';
      document.getElementById('timer').textContent = d.timer_readout || '--:--';
      document.getElementById('chords').textContent = d.chords_line || '';
      document.getElementById('curr').textContent = d.blackout ? '(BLACKOUT)' : (d.current_text || '');
      document.getElementById('next').textContent = d.next_text || '—';
      const a = document.getElementById('alert');
      if (d.stage_message) { a.style.display = 'block'; a.textContent = d.stage_message; } else { a.style.display = 'none'; }
    }
    const ws = new WebSocket((location.protocol === 'https:' ? 'wss://' : 'ws://') + location.host + '/ws/stage');
    ws.onmessage = (e) => apply(JSON.parse(e.data));
    fetch('/api/stage/state').then(r => r.json()).then(apply).catch(() => {});
  </script>
</body>
</html>"#;

#[cfg(test)]
mod tests {
    use super::*;
    use core_scene::{SceneDiff, SceneState};
    use http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[test]
    fn builds_payload_for_live_and_blackout_states() {
        let mut state = SceneState::demo_m0(2);
        let live = payload_from_scene(&state);
        assert!(!live.blackout);
        assert!(live.current_text.contains("WELCOME"));
        assert!(live.next_text.is_some());

        state.apply(SceneDiff::SetBlackout(true));
        let dark = payload_from_scene(&state);
        assert!(dark.blackout);
        assert!(dark.current_text.is_empty());
    }

    #[tokio::test]
    async fn serves_stage_html_json_updates_and_404_on_unknown_route() {
        let mut state = SceneState::demo_m0(2);
        let hub = StageHub::new(payload_from_scene(&state));
        let mut sub = hub.subscribe();
        let _standalone = build_stage_router(payload_from_scene(&state));
        let app = build_stage_router_with_hub(hub.clone());

        state.apply(SceneDiff::SetStageMessage(Some("Nursery #42".into())));
        hub.publish(payload_from_scene(&state));
        let streamed = sub.recv().await.expect("broadcast subscriber receives live update");
        assert_eq!(streamed.stage_message.as_deref(), Some("Nursery #42"));

        // Happy path 1: GET / returns Stage Display HTML
        let res1 = app
            .clone()
            .oneshot(Request::builder().uri("/").body(axum::body::Body::empty()).expect("req"))
            .await
            .expect("response 1");
        assert_eq!(res1.status(), StatusCode::OK);
        let body1 = res1.into_body().collect().await.expect("body 1").to_bytes();
        let html = String::from_utf8_lossy(&body1);
        assert!(html.contains("OpenPresenter — LAN Stage Display"));

        // Happy path 2: GET /api/stage/state returns live JSON payload with stage message
        let res2 = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/stage/state")
                    .body(axum::body::Body::empty())
                    .expect("req 2"),
            )
            .await
            .expect("response 2");
        assert_eq!(res2.status(), StatusCode::OK);
        let body2 = res2.into_body().collect().await.expect("body 2").to_bytes();
        let decoded: StageViewPayload = serde_json::from_slice(&body2).expect("valid json");
        assert_eq!(decoded.stage_message.as_deref(), Some("Nursery #42"));
        assert!(decoded.current_text.contains("WELCOME"));

        // Failure path: unknown endpoint returns 404 Not Found
        let res3 = app
            .oneshot(
                Request::builder()
                    .uri("/does-not-exist")
                    .body(axum::body::Body::empty())
                    .expect("req 3"),
            )
            .await
            .expect("response 3");
        assert_eq!(res3.status(), StatusCode::NOT_FOUND);
    }
}
