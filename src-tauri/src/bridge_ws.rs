// Localhost WebSocket bridge to the companion Chrome extension (Step 2).
//
// Amdion is always running, so it HOSTS the WS server and the extension
// connects to it (this beats Chrome Native Messaging, which would launch a
// per-connection subprocess fighting the already-running app). The server binds
// loopback only and speaks a small JSON envelope `{ type, payload }`:
//
//   Ext → App : hello, tab_opened / tab_activated / tab_closed / tab_navigated,
//               idle_state, ping
//   App → Ext : friction { level, blockList }, open_tab { url },
//               focus_tab { tabId }, close_tab { tabId }
//
// Commands push App→Ext messages by sending an already-serialized JSON string on
// `AppState.bridge_tx` (a broadcast channel); each connection's pump forwards it.
//
// Security: a localhost WS is reachable by any local process, INCLUDING any web
// page the user visits (`new WebSocket('ws://127.0.0.1:…')`). Two guards:
//   1. Handshake `Origin` must be a Chrome extension — browsers forbid JS from
//      forging `Origin`, so this blocks the web-page threat outright.
//   2. A shared `token` (in `bridge.json` + the `hello`) — plumbed now, enforced
//      later for the Web-Store build. The unpacked dev build trusts the pinned
//      extension origin (`ALLOWED_EXTENSION_ORIGIN`, set once the extension's
//      manifest `key` exists).

use crate::config::{app_data_dir, read_config};
use crate::state::AppState;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::{self, StatusCode};
use tokio_tungstenite::tungstenite::Message;

/// Port range Amdion and the extension both know. The app binds the first free
/// one; the extension scans the same range, so they rendezvous without the
/// extension needing to read a file (a background service worker can't).
const PORT_MIN: u16 = 17872;
const PORT_MAX: u16 = 17882;

/// Exact `chrome-extension://<id>` origin to allow. The id is pinned by the
/// `key` in `extension/manifest.json`, so only Amdion's own companion extension
/// can connect — not some other extension the user happens to have installed.
/// (`None` would fall back to accepting any `chrome-extension://` origin.)
const ALLOWED_EXTENSION_ORIGIN: Option<&str> =
    Some("chrome-extension://kobehecgjgjgjlljidhjjlgadpdmnfbp");

/// Whether the `hello` token must match. Off for the pinned-origin dev build;
/// the hardening hook for the Web-Store build (see module docs).
const REQUIRE_TOKEN: bool = false;

/// Incoming envelope from the extension. Re-serialized verbatim onto the
/// `browser-activity` Tauri event so the panel/observer can see raw activity.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct Envelope {
    #[serde(rename = "type")]
    typ: String,
    #[serde(default)]
    payload: serde_json::Value,
}

/// Bind the bridge and accept extension connections forever. Spawned from
/// `lib.rs` `setup()`.
pub async fn serve(app: AppHandle, tx: broadcast::Sender<String>, token: String, conns: Arc<AtomicUsize>) {
    let mut bound: Option<(TcpListener, u16)> = None;
    for port in PORT_MIN..=PORT_MAX {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)).await {
            bound = Some((listener, port));
            break;
        }
    }
    let Some((listener, port)) = bound else {
        eprintln!("[bridge] could not bind any port in {PORT_MIN}..={PORT_MAX}; Chrome bridge disabled");
        return;
    };
    write_discovery_file(port, &token);
    eprintln!("[bridge] listening on ws://127.0.0.1:{port}");

    loop {
        match listener.accept().await {
            Ok((stream, _peer)) => {
                tokio::spawn(handle_conn(stream, app.clone(), tx.clone(), token.clone(), conns.clone()));
            }
            Err(e) => eprintln!("[bridge] accept error: {e}"),
        }
    }
}

/// Drive one extension connection: origin-gate the handshake, authenticate the
/// `hello`, pump App→Ext broadcasts out, and route Ext→App events in.
async fn handle_conn(
    stream: TcpStream,
    app: AppHandle,
    tx: broadcast::Sender<String>,
    token: String,
    conns: Arc<AtomicUsize>,
) {
    let origin_gate = |req: &Request, response: Response| -> Result<Response, ErrorResponse> {
        if origin_allowed(req) {
            Ok(response)
        } else {
            let body = http::Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Some("amdion: forbidden origin".to_string()))
                .unwrap();
            Err(body)
        }
    };

    let ws = match tokio_tungstenite::accept_hdr_async(stream, origin_gate).await {
        Ok(ws) => ws,
        Err(_) => return, // rejected handshake or protocol error
    };
    let (mut write, mut read) = ws.split();

    // App→Ext: forward every broadcast frame. A lagging consumer drops the
    // oldest control message, which is harmless (state is re-derivable).
    let mut rx = tx.subscribe();
    let pump = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    if write.send(Message::text(msg)).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Ext→App: the first valid `hello` authenticates; everything after routes.
    let mut authed = false;
    while let Some(Ok(msg)) = read.next().await {
        match msg {
            Message::Text(txt) => {
                let Ok(env) = serde_json::from_str::<Envelope>(txt.as_str()) else {
                    continue;
                };
                if !authed {
                    if env.typ == "hello" && hello_ok(&env, &token) {
                        authed = true;
                        let n = conns.fetch_add(1, Ordering::SeqCst) + 1;
                        eprintln!("[bridge] extension connected ({n} active)");
                        broadcast_connected(&app, &conns);
                        // Configure the extension the instant it connects — the
                        // broadcast reaches our own freshly-subscribed `rx`.
                        let _ = tx.send(friction_message());
                    } else {
                        break; // refuse pre-auth traffic
                    }
                    continue;
                }
                route_event(&app, &env);
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    pump.abort();
    if authed {
        let n = conns.fetch_sub(1, Ordering::SeqCst) - 1;
        eprintln!("[bridge] extension disconnected ({n} active)");
        broadcast_connected(&app, &conns);
    }
}

/// True if the handshake `Origin` is an allowed Chrome extension.
fn origin_allowed(req: &Request) -> bool {
    let origin = req
        .headers()
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    match ALLOWED_EXTENSION_ORIGIN {
        Some(exact) => origin == exact,
        None => origin.starts_with("chrome-extension://"),
    }
}

/// Token check for the `hello`. A no-op until `REQUIRE_TOKEN` is enabled.
fn hello_ok(env: &Envelope, token: &str) -> bool {
    if !REQUIRE_TOKEN {
        return true;
    }
    env.payload.get("token").and_then(|t| t.as_str()) == Some(token)
}

/// Route an authenticated Ext→App event: persist it to the SQLite store (Step 3)
/// and surface it live on the `browser-activity` Tauri event. The OS sensing
/// thread writes `'os'` rows into the same `events` table; the classifier merges
/// both streams by timestamp.
fn route_event(app: &AppHandle, env: &Envelope) {
    match env.typ.as_str() {
        "tab_opened" | "tab_activated" | "tab_closed" | "tab_navigated" | "idle_state" => {
            persist_browser_event(app, env);
            let _ = app.emit("browser-activity", env);
        }
        "ping" => {} // keepalive only
        _ => {}
    }
}

/// Append a browser event to the event store as a `'browser'`-source row: `url`
/// is lifted out for direct querying; the whole payload rides in `meta`. `app`
/// is left null (the OS sensing thread owns the frontmost-app/bundle column).
fn persist_browser_event(app: &AppHandle, env: &Envelope) {
    let Some(db) = app.try_state::<crate::db::Db>() else {
        eprintln!("[bridge] db not ready; dropping '{}' event", env.typ);
        return;
    };
    let url = env.payload.get("url").and_then(|v| v.as_str());
    let meta = serde_json::to_string(&env.payload).ok();
    db.insert_event(&env.typ, "browser", None, url, meta.as_deref());
}

/// Set `extension_connected` from the live count and push `state-update`, so the
/// panel reflects connect/disconnect over the channel it already listens on.
fn broadcast_connected(app: &AppHandle, conns: &AtomicUsize) {
    let connected = conns.load(Ordering::SeqCst) > 0;
    if let Some(state) = app.try_state::<AppState>() {
        let data = {
            let mut d = state.data.lock().unwrap();
            d.extension_connected = connected;
            d.clone()
        };
        let _ = app.emit("state-update", &data);
    }
}

/// The current friction config as an App→Ext `friction` message.
pub fn friction_message() -> String {
    let cfg = read_config();
    serde_json::json!({
        "type": "friction",
        "payload": { "level": cfg.friction_level, "blockList": cfg.block_list },
    })
    .to_string()
}

/// Write `{ port, token }` to `bridge.json` in the app-data dir — a discovery/
/// debug aid (the extension scans the port range itself; it can't read files).
fn write_discovery_file(port: u16, token: &str) {
    let path = app_data_dir().join("bridge.json");
    let body = serde_json::json!({ "port": port, "token": token });
    if let Ok(json) = serde_json::to_string_pretty(&body) {
        let _ = std::fs::write(path, json);
    }
}
