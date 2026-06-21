// Amdion — the localhost WebSocket bridge to the desktop app.
//
// Owns the socket lifecycle and message I/O. Inbound App→Ext messages are routed
// by type to handlers registered with onBridge() (core and features register
// their own). Outbound goes through send(). MV3 service workers are ephemeral, so
// a chrome.alarms heartbeat reconnects and keeps a live socket warm (an open
// socket alone does NOT keep the worker alive).

// The app binds the first free port in ITS range; we scan dev ports FIRST so a
// running `tauri dev` instance always wins over the installed release app on the
// same machine (SEPARATE ranges — see src-tauri/src/bridge_ws.rs). DEV/REL must
// stay in sync with the debug_assertions split there.
const DEV_PORTS = Array.from({ length: 11 }, (_, i) => 17883 + i); // dev:     17883–17893
const REL_PORTS = Array.from({ length: 11 }, (_, i) => 17872 + i); // release: 17872–17882
const PORTS = [...DEV_PORTS, ...REL_PORTS];
const DEV_PORT_SET = new Set(DEV_PORTS);

const KEEPALIVE = 'amdion-keepalive';
const EXT_VERSION = chrome.runtime.getManifest().version;

let ws = null;
let portIdx = 0;
let connected = false;
let connectedPort = null; // the port the live socket is on (tells dev vs release)
let reconnectTimer = null;
let migrateProbe = null;  // throwaway socket used to detect a dev instance

const inbound = new Map(); // msg.type -> handler(payload, msg)

// Register an App→Ext message handler. Last registration for a type wins.
export function onBridge(type, handler) { inbound.set(type, handler); }

export function isConnected() { return connected; }

export function send(obj) {
  if (ws && ws.readyState === WebSocket.OPEN) {
    try { ws.send(JSON.stringify(obj)); } catch (_) {}
  }
}

export function connect() {
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return;
  const port = PORTS[portIdx % PORTS.length];
  try {
    ws = new WebSocket(`ws://127.0.0.1:${port}`);
  } catch (_) {
    scheduleReconnect();
    return;
  }
  ws.onopen = () => {
    connected = true;
    connectedPort = port;
    send({ type: 'hello', payload: { extVersion: EXT_VERSION, browser: 'chrome' } });
  };
  ws.onmessage = (ev) => handleMessage(ev.data);
  ws.onclose = () => {
    connected = false;
    connectedPort = null;
    ws = null;
    portIdx++; // try the next port in the (dev-first) range on the next attempt
    scheduleReconnect();
  };
  ws.onerror = () => { try { ws.close(); } catch (_) {} };
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => { reconnectTimer = null; connect(); }, 1500);
}

function handleMessage(data) {
  let msg;
  try { msg = JSON.parse(data); } catch (_) { return; }
  const handler = inbound.get(msg.type);
  if (!handler) return;
  try { handler(msg.payload || {}, msg); }
  catch (e) { console.warn('[amdion] bridge handler', msg.type, 'failed:', e); }
}

// While connected to a RELEASE port, periodically check whether a dev instance
// has come up; if so, migrate to it. Non-destructive: tear down the live release
// socket only once a dev port actually accepts. Bounded: at most one in-flight
// probe, ≤11 dev ports, once per keepalive tick, instant no-op once on dev.
function maybeMigrateToDev() {
  if (!ws || ws.readyState !== WebSocket.OPEN) return;
  if (connectedPort == null || DEV_PORT_SET.has(connectedPort)) return;
  if (migrateProbe) return;

  let idx = 0;
  const tryNext = () => {
    if (idx >= DEV_PORTS.length) { migrateProbe = null; return; }
    const port = DEV_PORTS[idx++];
    let probe;
    try { probe = new WebSocket(`ws://127.0.0.1:${port}`); }
    catch (_) { tryNext(); return; }
    migrateProbe = probe;
    probe.onopen = () => {
      // A dev instance is up. Drop the probe and the release socket; the cursor,
      // reset to the top of the (dev-first) range, reconnects us to dev.
      try { probe.close(); } catch (_) {}
      migrateProbe = null;
      portIdx = 0;
      try { if (ws) ws.close(); } catch (_) {} // onclose schedules the reconnect
    };
    probe.onerror = () => { try { probe.close(); } catch (_) {} };
    probe.onclose = () => { if (migrateProbe === probe) { migrateProbe = null; tryNext(); } };
  };
  tryNext();
}

// Keepalive: reconnect a dead socket, ping a live one, re-probe for dev.
chrome.alarms.create(KEEPALIVE, { periodInMinutes: 0.4 });
chrome.alarms.onAlarm.addListener((a) => {
  if (a.name !== KEEPALIVE) return;
  if (!ws || ws.readyState === WebSocket.CLOSED || ws.readyState === WebSocket.CLOSING) {
    connect();
  } else if (ws.readyState === WebSocket.OPEN) {
    send({ type: 'ping' });
    maybeMigrateToDev(); // cheap dev re-probe while parked on a release port
  }
});
