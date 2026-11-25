//! Embedded `axum` HTTP + WebSocket LAN Browser Stage Display server.
//!
//! Lets worship leaders and musicians view live Current Slide, Next Slide, Chords, Timer,
//! and urgent Stage Messages from any tablet/phone on sanctuary Wi-Fi without touching
//! the native `wgpu` Audience or Stage outputs.

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxumPath, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Json};
use axum::routing::{get, post};
use core_scene::{ChordChartDocument, SceneState, SheetMusicDocument};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tokio::net::TcpListener;
use tokio::sync::broadcast;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteSlideSummary {
    pub id: String,
    pub label: String,
    pub group: String,
    pub active: bool,
}

/// Live JSON payload pushed to connected LAN browser Stage Display, Musician Display, & Remote clients.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageViewPayload {
    pub revision: u64,
    pub blackout: bool,
    #[serde(default)]
    pub freeze_output: bool,
    pub current_label: String,
    #[serde(default)]
    pub current_group: String,
    #[serde(default)]
    pub song_title: Option<String>,
    pub current_text: String,
    pub chords_line: Option<String>,
    pub next_text: Option<String>,
    pub stage_message: Option<String>,
    pub timer_readout: Option<String>,
    #[serde(default)]
    pub deck: Vec<RemoteSlideSummary>,
    #[serde(default)]
    pub chord_chart: Option<ChordChartDocument>,
    #[serde(default)]
    pub sheet_music: Option<SheetMusicDocument>,
    #[serde(default)]
    pub sheet_active_page: u32,
}

/// Interactive command sent from the PIN-authenticated LAN Mobile Web Remote Controller (`/remote`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum RemoteCommand {
    NextSlide,
    PrevSlide,
    TriggerSlide { slide_id: String },
    SetBlackout { enabled: bool },
    SetFreeze { enabled: bool },
    ClearAll,
    SetStageMessage { message: Option<String> },
    SetPointer { x: f32, y: f32, active: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteCommandRequest {
    pub pin: String,
    #[serde(flatten)]
    pub command: RemoteCommand,
}

pub type RemoteCommandHandler = Arc<dyn Fn(RemoteCommand) + Send + Sync>;

/// Builds a [`StageViewPayload`] snapshot directly from canonical [`SceneState`].
pub fn payload_from_scene(state: &SceneState) -> StageViewPayload {
    let (current_label, current_group, current_text, chords_line) = match &state.active_slide {
        Some(slide) if state.slide_layer_active && !state.blackout => (
            slide.label.clone(),
            slide.group.clone(),
            slide.plain_text(),
            slide.chords_line.clone(),
        ),
        _ => (String::new(), String::new(), String::new(), None),
    };

    let next_text = state
        .next_slide_preview
        .as_ref()
        .map(|s| s.plain_text().replace('\n', " / "));

    let active_id = state.active_slide.as_ref().map(|s| s.id);
    let active_idx = active_id
        .and_then(|id| state.deck.iter().position(|s| s.id == id))
        .unwrap_or(0);
    let sheet_active_page = state
        .active_sheet_music
        .as_ref()
        .map(|sm| sm.page_for_slide(state.active_slide.as_ref(), active_idx, state.deck.len()))
        .unwrap_or(1);

    let deck = state
        .deck
        .iter()
        .map(|s| RemoteSlideSummary {
            id: s.id.0.to_string(),
            label: s.label.clone(),
            group: s.group.clone(),
            active: Some(s.id) == active_id,
        })
        .collect();

    StageViewPayload {
        revision: state.revision,
        blackout: state.blackout,
        freeze_output: state.freeze_output,
        current_label,
        current_group,
        song_title: state.active_presentation_title.clone(),
        current_text,
        chords_line,
        next_text,
        stage_message: state.stage_message.clone(),
        timer_readout: state.timer_readout.clone(),
        deck,
        chord_chart: state.active_chord_chart.clone(),
        sheet_music: state.active_sheet_music.clone(),
        sheet_active_page,
    }
}

/// Publish/subscribe state hub backing Stage Display & PIN-authenticated Remote Control clients.
#[derive(Clone)]
pub struct StageHub {
    latest: Arc<RwLock<StageViewPayload>>,
    tx: broadcast::Sender<StageViewPayload>,
    remote_pin: Arc<RwLock<String>>,
    handler: Option<RemoteCommandHandler>,
}

impl StageHub {
    pub fn new(initial: StageViewPayload) -> Self {
        Self::with_remote(initial, "2026".to_string(), None)
    }

    pub fn with_remote(
        initial: StageViewPayload,
        pin: String,
        handler: Option<RemoteCommandHandler>,
    ) -> Self {
        let (tx, _) = broadcast::channel(32);
        Self {
            latest: Arc::new(RwLock::new(initial)),
            tx,
            remote_pin: Arc::new(RwLock::new(pin)),
            handler,
        }
    }

    pub fn remote_pin(&self) -> String {
        self.remote_pin
            .read()
            .map(|g| g.clone())
            .unwrap_or_else(|_| "2026".to_string())
    }

    pub fn latest(&self) -> StageViewPayload {
        self.latest
            .read()
            .map(|g| g.clone())
            .unwrap_or(StageViewPayload {
                revision: 0,
                blackout: true,
                freeze_output: false,
                current_label: String::new(),
                current_group: String::new(),
                song_title: None,
                current_text: String::new(),
                chords_line: None,
                next_text: None,
                stage_message: None,
                timer_readout: None,
                deck: Vec::new(),
                chord_chart: None,
                sheet_music: None,
                sheet_active_page: 1,
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

/// Handle to a running LAN Browser Stage Display & Mobile Remote Control server.
pub struct WebStageServer {
    addr: SocketAddr,
    hub: StageHub,
}

impl WebStageServer {
    pub async fn bind(bind_addr: &str, initial: StageViewPayload) -> Result<Self, std::io::Error> {
        Self::bind_with_remote(bind_addr, initial, "2026".to_string(), None).await
    }

    pub async fn bind_with_remote(
        bind_addr: &str,
        initial: StageViewPayload,
        pin: String,
        handler: Option<RemoteCommandHandler>,
    ) -> Result<Self, std::io::Error> {
        let listener = TcpListener::bind(bind_addr).await?;
        let addr = listener.local_addr()?;
        let hub = StageHub::with_remote(initial, pin, handler);
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

    pub fn remote_url(&self) -> String {
        format!("http://{}/remote", self.addr)
    }

    pub fn musician_url(&self) -> String {
        format!("http://{}/musician", self.addr)
    }

    pub fn remote_pin(&self) -> String {
        self.hub.remote_pin()
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
        .route("/stage", get(stage_html))
        .route("/musician", get(musician_html))
        .route("/remote", get(remote_html))
        .route("/favicon.svg", get(favicon_svg))
        .route("/api/stage/state", get(stage_json))
        .route("/api/stage/sheet-page/{page}", get(sheet_page_image))
        .route("/api/remote/command", post(remote_command))
        .route("/ws/stage", get(stage_ws))
        .with_state(hub)
}

async fn favicon_svg() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "image/svg+xml")],
        include_str!("../../../docs/images/logo.svg"),
    )
}

async fn stage_html() -> Html<&'static str> {
    Html(STAGE_DISPLAY_HTML)
}

async fn musician_html() -> Html<&'static str> {
    Html(MUSICIAN_STAGE_HTML)
}

async fn remote_html() -> Html<&'static str> {
    Html(REMOTE_CONTROL_HTML)
}

async fn sheet_page_image(
    AxumPath(page_num): AxumPath<u32>,
    State(hub): State<StageHub>,
) -> impl IntoResponse {
    let latest = hub.latest();
    let Some(sheet) = latest.sheet_music else {
        return (StatusCode::NOT_FOUND, "No sheet music active").into_response();
    };
    let Some(page) = sheet
        .pages
        .iter()
        .find(|p| p.page_number == page_num)
        .and_then(|p| p.image_path.as_deref())
    else {
        return (StatusCode::NOT_FOUND, "Page image not available").into_response();
    };
    let Ok(bytes) = std::fs::read(page) else {
        return (StatusCode::NOT_FOUND, "Sheet image unreadable").into_response();
    };
    let content_type = if page.to_ascii_lowercase().ends_with(".png") {
        "image/png"
    } else if page.to_ascii_lowercase().ends_with(".bmp") {
        "image/bmp"
    } else {
        "image/jpeg"
    };
    ([(header::CONTENT_TYPE, content_type)], bytes).into_response()
}

async fn stage_json(State(hub): State<StageHub>) -> Json<StageViewPayload> {
    Json(hub.latest())
}

async fn remote_command(
    State(hub): State<StageHub>,
    Json(req): Json<RemoteCommandRequest>,
) -> impl IntoResponse {
    if req.pin.trim() != hub.remote_pin() {
        return (StatusCode::UNAUTHORIZED, "Invalid PIN").into_response();
    }
    if let Some(handler) = &hub.handler {
        handler(req.command);
    }
    (StatusCode::OK, "OK").into_response()
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
<link rel="icon" type="image/svg+xml" href="/favicon.svg" />
<title>TrueShow — LAN Stage Display</title>
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
  <header><span id="label">TrueShow Stage</span><span id="timer">--:--</span></header>
  <main><div id="chords"></div><div id="curr">Waiting for cue...</div></main>
  <div id="alert"></div>
  <footer>Next: <span id="next">—</span></footer>
  <script>
    function apply(d) {
      document.getElementById('label').textContent = d.current_label || 'TrueShow Stage';
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

const REMOTE_CONTROL_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width,initial-scale=1" />
<link rel="icon" type="image/svg+xml" href="/favicon.svg" />
<title>TrueShow — Mobile Remote Controller</title>
<style>
  body { margin:0; background:#0b0f17; color:#f4f7fc; font-family:system-ui,sans-serif; padding:12px; display:flex; flex-direction:column; gap:10px; }
  .bar { display:flex; gap:8px; flex-wrap:wrap; align-items:center; }
  input, button { padding:10px 12px; border-radius:8px; border:1px solid #2b354b; background:#141b29; color:#fff; font-weight:600; font-size:14px; }
  button.primary { background:#2563eb; border-color:#3b82f6; }
  button.danger { background:#991b1b; border-color:#ef4444; }
  #slides { display:grid; grid-template-columns:repeat(auto-fill,minmax(150px,1fr)); gap:8px; }
  .slide { text-align:left; padding:10px; }
  .slide.live { border-color:#22c55e; background:#14532d; }
  #pad { height:120px; background:#070a11; border:1px dashed #3b82f6; border-radius:8px; display:grid; place-items:center; color:#93c5fd; touch-action:none; }
</style>
</head>
<body>
  <div class="bar">
    <strong>TrueShow Remote</strong>
    <input id="pin" placeholder="PIN (2026)" value="2026" style="width:110px" />
    <button class="primary" onclick="send({action:'prev_slide'})">◀ Prev</button>
    <button class="primary" onclick="send({action:'next_slide'})">Next ▶</button>
    <button onclick="send({action:'set_freeze',enabled:!st.freeze_output})">Freeze</button>
    <button class="danger" onclick="send({action:'set_blackout',enabled:!st.blackout})">Blackout</button>
    <button onclick="send({action:'clear_all'})">Clear All</button>
  </div>
  <div id="slides"></div>
  <div id="pad">Drag here for Live Laser Pointer</div>
  <script>
    let st = { blackout:false, freeze_output:false, deck:[] };
    async function send(cmd) {
      const pin = document.getElementById('pin').value;
      await fetch('/api/remote/command', { method:'POST', headers:{'content-type':'application/json'}, body: JSON.stringify({ pin, ...cmd }) });
    }
    function render(d) {
      st = d;
      const el = document.getElementById('slides');
      el.innerHTML = '';
      (d.deck || []).forEach(s => {
        const b = document.createElement('button');
        b.className = 'slide' + (s.active ? ' live' : '');
        b.textContent = s.label;
        b.onclick = () => send({ action:'trigger_slide', slide_id: s.id });
        el.appendChild(b);
      });
    }
    const pad = document.getElementById('pad');
    pad.onpointermove = (e) => { if (e.buttons) { const r = pad.getBoundingClientRect(); send({ action:'set_pointer', x:(e.clientX-r.left)/r.width, y:(e.clientY-r.top)/r.height, active:true }); } };
    pad.onpointerup = () => send({ action:'set_pointer', x:0, y:0, active:false });
    const ws = new WebSocket((location.protocol === 'https:' ? 'wss://' : 'ws://') + location.host + '/ws/stage');
    ws.onmessage = (e) => render(JSON.parse(e.data));
    fetch('/api/stage/state').then(r => r.json()).then(render).catch(() => {});
  </script>
</body>
</html>"#;

const MUSICIAN_STAGE_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width,initial-scale=1" />
<link rel="icon" type="image/svg+xml" href="/favicon.svg" />
<title>TrueShow — Musician Chord Chart & Sheet Music Stage</title>
<style>
  body { margin:0; background:#060911; color:#f1f5f9; font-family:system-ui,sans-serif; display:flex; flex-direction:column; height:100vh; }
  header { display:flex; flex-wrap:wrap; align-items:center; justify-content:space-between; gap:10px; padding:10px 18px; background:#0f172a; border-bottom:2px solid #0284c7; }
  .meta { font-weight:700; font-size:18px; }
  .controls { display:flex; gap:8px; align-items:center; font-size:13px; }
  .controls button, .controls select { background:#1e293b; color:#fff; border:1px solid #334155; border-radius:6px; padding:5px 10px; font-weight:600; cursor:pointer; }
  .controls button.active { background:#0284c7; border-color:#38bdf8; }
  #timer { color:#34d399; font-weight:700; font-size:20px; font-variant-numeric:tabular-nums; }
  #roadmap { padding:8px 18px; background:#0b1220; border-bottom:1px solid #1e293b; display:flex; gap:8px; flex-wrap:wrap; font-size:13px; }
  .pill { padding:3px 10px; border-radius:999px; background:#1e293b; color:#94a3b8; }
  .pill.now { background:#0284c7; color:#fff; font-weight:700; }
  main { flex:1; overflow:auto; padding:14px 18px; display:grid; grid-template-columns:1.15fr 1fr; gap:14px; }
  main.sheet-mode { grid-template-columns:1fr 1fr; }
  main.sheet-single { grid-template-columns:1fr; }
  .panel { background:#0d1424; border:1px solid #1e293b; border-radius:10px; padding:14px; }
  .panel.active { border:2px solid #38bdf8; background:#0f1a2e; }
  .sec-head { font-size:14px; font-weight:800; color:#38bdf8; text-transform:uppercase; margin-bottom:10px; }
  .chords { color:#fbbf24; font-family:ui-monospace,monospace; font-size:20px; font-weight:700; white-space:pre; }
  .lyrics { color:#fff; font-family:ui-monospace,monospace; font-size:20px; margin-bottom:10px; white-space:pre-wrap; }
  .sheet-card { background:#f8fafc; color:#0f172a; border-radius:10px; padding:10px; display:flex; flex-direction:column; align-items:center; justify-content:center; min-height:70vh; }
  .sheet-card.invert { filter:invert(1) hue-rotate(180deg); }
  .sheet-card img { max-width:100%; max-height:76vh; object-fit:contain; }
  #alert { display:none; padding:10px 18px; background:#dc2626; color:#fff; font-size:20px; font-weight:700; text-align:center; }
</style>
</head>
<body>
  <header>
    <div class="meta" id="title">TrueShow Musician Stage</div>
    <div class="controls">
      <button id="btn-chords" class="active" onclick="setMode('chords')">Chord Chart</button>
      <button id="btn-sheet" onclick="setMode('sheet')">Sheet Music</button>
      <label>Transpose: <select id="transpose" onchange="render()">
        <option value="-6">-6</option><option value="-5">-5</option><option value="-4">-4</option>
        <option value="-3">-3</option><option value="-2">-2</option><option value="-1">-1</option>
        <option value="0" selected>0</option><option value="1">+1</option><option value="2">+2</option>
        <option value="3">+3</option><option value="4">+4</option><option value="5">+5</option><option value="6">+6</option>
      </select></label>
      <button id="btn-spread" onclick="toggleSpread()">2-Page Spread: ON</button>
      <button id="btn-invert" onclick="toggleInvert()">Dark Paper: OFF</button>
      <span id="timer">--:--</span>
    </div>
  </header>
  <div id="roadmap"></div>
  <main id="content"></main>
  <div id="alert"></div>
  <script>
    const SHARPS=['C','C#','D','D#','E','F','F#','G','G#','A','A#','B'];
    const FLATS=['C','Db','D','Eb','E','F','Gb','G','Ab','A','Bb','B'];
    let latest=null, mode='chords', twoPage=true, darkInvert=false;
    function tNote(tok,semi){
      if(!semi) return tok;
      if(tok.includes('/')) return tok.split('/').map(p=>tNote(p,semi)).join('/');
      const m=/^([A-G])(#|b)?(.*)$/.exec(tok); if(!m) return tok;
      const base={C:0,D:2,E:4,F:5,G:7,A:9,B:11}[m[1]];
      const idx=(base+(m[2]==='#'?1:m[2]==='b'?-1:0)+semi+120)%12;
      const tbl=(m[2]==='b'||semi<0)?FLATS:SHARPS;
      return tbl[idx]+m[3];
    }
    function tLine(line,semi){ return !semi?line:line.replace(/[A-G][#b]?[a-zA-Z0-9+/]*/g,t=>tNote(t,semi)); }
    function setMode(m){ mode=m; document.getElementById('btn-chords').classList.toggle('active',m==='chords'); document.getElementById('btn-sheet').classList.toggle('active',m==='sheet'); render(); }
    function toggleSpread(){ twoPage=!twoPage; document.getElementById('btn-spread').textContent='2-Page Spread: '+(twoPage?'ON':'OFF'); render(); }
    function toggleInvert(){ darkInvert=!darkInvert; document.getElementById('btn-invert').textContent='Dark Paper: '+(darkInvert?'ON':'OFF'); render(); }
    function render(){
      if(!latest) return;
      const semi=Number(document.getElementById('transpose').value||0);
      const cc=latest.chord_chart, sm=latest.sheet_music;
      document.getElementById('title').textContent=(latest.song_title||latest.current_label||'Musician Stage')+(cc&&cc.key_signature?' · Key '+tNote(cc.key_signature,semi):'')+(cc&&cc.tempo_bpm?' · '+Math.round(cc.tempo_bpm)+' BPM':'');
      document.getElementById('timer').textContent=latest.timer_readout||'--:--';
      const al=document.getElementById('alert');
      if(latest.stage_message){al.style.display='block';al.textContent=latest.stage_message;}else{al.style.display='none';}
      const main=document.getElementById('content');
      const road=document.getElementById('roadmap');
      if(mode==='sheet'){
        main.className=twoPage?'sheet-mode':'sheet-single';
        const total=sm?sm.pages.length:0, cur=latest.sheet_active_page||1;
        road.innerHTML='<span class="pill now">SYNC: '+(latest.current_group||latest.current_label||'READY')+'</span><span class="pill">Page '+cur+' of '+Math.max(1,total)+'</span>';
        const pages=twoPage&&cur<total?[cur,cur+1]:[cur];
        main.innerHTML=pages.map(p=>{
          const pg=sm&&sm.pages[p-1];
          const body=pg&&pg.image_path?'<img src="/api/stage/sheet-page/'+p+'?rev='+latest.revision+'" alt="Page '+p+'" />':'<div style="padding:24px;text-align:center"><h3>'+(pg?pg.label:'No Sheet Music Attached')+'</h3><p>'+(pg&&pg.staff_lines_hint.length?pg.staff_lines_hint.join('<br/>'):'Import a Sheet Music PDF or Page Images in Tools → Import & PCO')+'</p></div>';
          return '<div class="sheet-card'+(darkInvert?' invert':'')+'">'+body+'</div>';
        }).join('');
      } else {
        main.className='';
        const secs=cc&&cc.sections.length?cc.sections:[{group:latest.current_group||'Live',lines:[{chords:latest.chords_line,lyrics:latest.current_text}]}];
        const grp=(latest.current_group||'').toLowerCase();
        const curIdx=Math.max(0,secs.findIndex(s=>s.group.toLowerCase()===grp||s.group.toLowerCase().startsWith(grp)));
        road.innerHTML=secs.map((s,i)=>'<span class="pill'+(i===curIdx?' now':'')+'">'+s.group+'</span>').join('');
        const drawSec=(s,active)=>'<div class="panel'+(active?' active':'')+'"><div class="sec-head">'+(active?'▶ NOW: ':'NEXT: ')+s.group+'</div>'+s.lines.map(l=>(l.chords?'<div class="chords">'+tLine(l.chords,semi)+'</div>':'')+(l.lyrics?'<div class="lyrics">'+l.lyrics+'</div>':'')).join('')+'</div>';
        const upcoming=secs.slice(curIdx+1,curIdx+4).map(s=>drawSec(s,false)).join('');
        main.innerHTML=drawSec(secs[curIdx],true)+'<div>'+upcoming+'</div>';
      }
    }
    fetch('/api/stage/state').then(r=>r.json()).then(d=>{latest=d;render();});
    const proto=location.protocol==='https:'?'wss':'ws';
    const ws=new WebSocket(proto+'://'+location.host+'/ws/stage');
    ws.onmessage=(e)=>{latest=JSON.parse(e.data);render();};
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
        let streamed = sub
            .recv()
            .await
            .expect("broadcast subscriber receives live update");
        assert_eq!(streamed.stage_message.as_deref(), Some("Nursery #42"));

        // Happy path 1: GET / returns Stage Display HTML
        let res1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(axum::body::Body::empty())
                    .expect("req"),
            )
            .await
            .expect("response 1");
        assert_eq!(res1.status(), StatusCode::OK);
        let body1 = res1.into_body().collect().await.expect("body 1").to_bytes();
        let html = String::from_utf8_lossy(&body1);
        assert!(html.contains("TrueShow — LAN Stage Display"));

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

        // Remote Controller HTML + PIN authentication (401 invalid PIN, 200 valid PIN + handler)
        let received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let rec_clone = Arc::clone(&received);
        let remote_hub = StageHub::with_remote(
            payload_from_scene(&state),
            "4321".to_string(),
            Some(Arc::new(move |cmd| {
                if let Ok(mut g) = rec_clone.lock() {
                    g.push(cmd);
                }
            })),
        );
        let remote_app = build_stage_router_with_hub(remote_hub);
        let remote_page = remote_app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/remote")
                    .body(axum::body::Body::empty())
                    .expect("req"),
            )
            .await
            .expect("remote html");
        assert_eq!(remote_page.status(), StatusCode::OK);

        let bad_pin = remote_app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/remote/command")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"pin":"0000","action":"next_slide"}"#,
                    ))
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(bad_pin.status(), StatusCode::UNAUTHORIZED);

        let good_pin = remote_app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/remote/command")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"pin":"4321","action":"next_slide"}"#,
                    ))
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(good_pin.status(), StatusCode::OK);
        assert_eq!(received.lock().expect("lock").len(), 1);

        // Musician LAN Stage Display HTML + Sheet Music Page Image endpoint
        let tmp_png = std::env::temp_dir().join(format!("op-web-sheet-{}.png", std::process::id()));
        std::fs::write(&tmp_png, [0x89u8, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]).expect("png");
        let mut sheet_state = SceneState::demo_m0(2);
        sheet_state.apply(core_scene::SceneDiff::SetActiveMusicSheets {
            chord_chart: None,
            sheet_music: Some(SheetMusicDocument {
                title: "Piano Score".into(),
                source_path: None,
                pages: vec![core_scene::SheetMusicPage {
                    page_number: 1,
                    label: "Page 1".into(),
                    image_path: Some(tmp_png.to_string_lossy().to_string()),
                    staff_lines_hint: Vec::new(),
                }],
                group_to_page: std::collections::BTreeMap::new(),
                slide_to_page: std::collections::BTreeMap::new(),
            }),
        });
        let m_app = build_stage_router(payload_from_scene(&sheet_state));
        let m_html = m_app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/musician")
                    .body(axum::body::Body::empty())
                    .expect("req"),
            )
            .await
            .expect("musician html");
        assert_eq!(m_html.status(), StatusCode::OK);

        let fav = m_app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/favicon.svg")
                    .body(axum::body::Body::empty())
                    .expect("req"),
            )
            .await
            .expect("favicon");
        assert_eq!(fav.status(), StatusCode::OK);
        assert_eq!(
            fav.headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("image/svg+xml")
        );

        let p1 = m_app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/stage/sheet-page/1")
                    .body(axum::body::Body::empty())
                    .expect("req"),
            )
            .await
            .expect("page 1");
        assert_eq!(p1.status(), StatusCode::OK);
        let p9 = m_app
            .oneshot(
                Request::builder()
                    .uri("/api/stage/sheet-page/9")
                    .body(axum::body::Body::empty())
                    .expect("req"),
            )
            .await
            .expect("page 9");
        assert_eq!(p9.status(), StatusCode::NOT_FOUND);
        let _ = std::fs::remove_file(&tmp_png);
    }
}
