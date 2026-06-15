// Amdion companion — MV3 service worker.
//
// Connects to the always-running Amdion app over the localhost WebSocket bridge,
// reports tab/idle activity, applies the user's friction (Soft nudge / Lock-In
// block), and opens/focuses tabs on command. MV3 service workers are ephemeral,
// so a `chrome.alarms` heartbeat wakes us to reconnect and to keep a live socket
// warm (an open socket alone does NOT keep an MV3 worker alive).

// The app binds the first free port in this range; we scan the same range.
const PORTS = Array.from({ length: 11 }, (_, i) => 17872 + i);

// Built-in distraction set. Soft nudges and Lock-In act on this ∪ the user's
// block list (pushed by the app). Editable additions live in Amdion's settings.
const BUILTIN_DISTRACTIONS = [
  'youtube.com', 'twitter.com', 'x.com', 'facebook.com', 'instagram.com',
  'reddit.com', 'tiktok.com', 'netflix.com', 'twitch.tv',
];

// Reserved id range for our dynamic blocking rules, so we only ever remove ours.
const RULE_BASE = 9000;
const KEEPALIVE = 'amdion-keepalive';
const EXT_VERSION = chrome.runtime.getManifest().version;

let ws = null;
let portIdx = 0;
let connected = false;
let reconnectTimer = null;

// ── WebSocket lifecycle ───────────────────────────────────────────────────

function connect() {
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
    send({ type: 'hello', payload: { extVersion: EXT_VERSION, browser: 'chrome' } });
  };
  ws.onmessage = (ev) => handleMessage(ev.data);
  ws.onclose = () => {
    connected = false;
    ws = null;
    portIdx++; // try the next port in the range on the next attempt
    scheduleReconnect();
  };
  ws.onerror = () => { try { ws.close(); } catch (_) {} };
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => { reconnectTimer = null; connect(); }, 1500);
}

function send(obj) {
  if (ws && ws.readyState === WebSocket.OPEN) {
    try { ws.send(JSON.stringify(obj)); } catch (_) {}
  }
}

function handleMessage(data) {
  let msg;
  try { msg = JSON.parse(data); } catch (_) { return; }
  switch (msg.type) {
    case 'friction': applyFriction(msg.payload || {}); break;
    case 'open_tab': if (msg.payload && msg.payload.url) chrome.tabs.create({ url: msg.payload.url }); break;
    case 'focus_tab': focusTab(msg.payload && msg.payload.tabId); break;
    case 'close_tab': closeTab(msg.payload && msg.payload.tabId); break;
    case 'read_mode': applyReadMode(msg.payload || {}); break;
    case 'read_prefs': applyReadPrefs(msg.payload || {}); break;
    default: break;
  }
}

// `tabId` from the app may be Amdion's internal id (not a Chrome tab id) until
// the Step-3 id map exists — coerce and bail unless it's a real integer id.
function focusTab(id) {
  const n = Number(id);
  if (!Number.isInteger(n)) return;
  chrome.tabs.update(n, { active: true }, () => void chrome.runtime.lastError);
  chrome.tabs.get(n, (t) => {
    if (!chrome.runtime.lastError && t) {
      chrome.windows.update(t.windowId, { focused: true }, () => void chrome.runtime.lastError);
    }
  });
}

function closeTab(id) {
  const n = Number(id);
  if (!Number.isInteger(n)) return;
  chrome.tabs.remove(n, () => void chrome.runtime.lastError);
}

// ── Friction: store for content scripts + (re)build Lock-In blocking ────────

async function applyFriction(payload) {
  const level = payload.level || 'off';
  const blockList = Array.isArray(payload.blockList) ? payload.blockList : [];
  const distractions = [...new Set([...BUILTIN_DISTRACTIONS, ...blockList])];
  // Content scripts read these (and react to storage changes) for Soft nudges.
  await chrome.storage.local.set({ friction: { level, blockList }, distractions });
  await rebuildBlockingRules(level, distractions);
}

async function rebuildBlockingRules(level, distractions) {
  const existing = await chrome.declarativeNetRequest.getDynamicRules();
  const ourIds = existing
    .filter((r) => r.id >= RULE_BASE && r.id < RULE_BASE + 1000)
    .map((r) => r.id);

  let addRules = [];
  if (level === 'lockin') {
    addRules = distractions.map((domain, i) => ({
      id: RULE_BASE + i,
      priority: 1,
      action: {
        type: 'redirect',
        redirect: { url: chrome.runtime.getURL('blocked.html') + '?d=' + encodeURIComponent(domain) },
      },
      // requestDomains matches the domain and its subdomains (www., m., …).
      condition: { requestDomains: [domain], resourceTypes: ['main_frame'] },
    }));
  }
  await chrome.declarativeNetRequest.updateDynamicRules({ removeRuleIds: ourIds, addRules });
}

// ── Read Mode: enter/exit the in-page reader ────────────────────────────────
//
// The reader lives in content/reader.js. Three triggers funnel here:
//   • the Alt+Shift+R hotkey (chrome.commands, below),
//   • the app's "Read this tab" (App→Ext `read_mode`), and
//   • the in-page pill (handled entirely in the content script).
// We just tell the right tab to enter/exit; the content script does the rest.

function applyReadMode(payload) {
  const enter = payload.on !== false;
  const type = enter ? 'amdion-read-enter' : 'amdion-read-exit';
  const explicit = Number(payload.tabId);
  if (Number.isInteger(explicit)) sendToTab(explicit, type);
  else withActiveTab((id) => sendToTab(id, type));
}

function withActiveTab(fn) {
  chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
    const tab = tabs && tabs[0];
    if (tab && Number.isInteger(tab.id)) fn(tab.id);
  });
}

function sendToTab(tabId, type) {
  chrome.tabs.sendMessage(tabId, { type }, () => void chrome.runtime.lastError);
}

// App→Ext: mirror reading prefs into chrome.storage.local where reader.js reads
// them. reader.js watches storage and live-applies (theme, size, pill, …).
function applyReadPrefs(payload) {
  chrome.storage.local.set({ reading: payload || {} });
}

// The hotkey fires in the worker; relay it to whatever tab is in front.
chrome.commands.onCommand.addListener((command) => {
  if (command === 'toggle-read-mode') withActiveTab((id) => sendToTab(id, 'amdion-read-enter'));
});

// content/reader.js → app: forward read_started / read_ended over the bridge so
// the app can do the "wrap" (lock other tabs, log reading time). The app routes
// these like any other Ext→App event (bridge_ws.rs).
chrome.runtime.onMessage.addListener((msg) => {
  if (msg && msg.type === 'amdion-read-event') {
    send({ type: msg.event === 'started' ? 'read_started' : 'read_ended', payload: msg.payload || {} });
  }
});

// ── Activity reporting ──────────────────────────────────────────────────────

const tabInfo = (t) => ({ tabId: t.id, url: t.url, title: t.title, windowId: t.windowId });

chrome.tabs.onCreated.addListener((t) => send({ type: 'tab_opened', payload: tabInfo(t) }));
chrome.tabs.onRemoved.addListener((tabId) => send({ type: 'tab_closed', payload: { tabId } }));
chrome.tabs.onActivated.addListener(({ tabId }) => {
  chrome.tabs.get(tabId, (t) => { if (!chrome.runtime.lastError && t) send({ type: 'tab_activated', payload: tabInfo(t) }); });
});
chrome.webNavigation.onCommitted.addListener((d) => {
  if (d.frameId === 0) send({ type: 'tab_navigated', payload: { tabId: d.tabId, url: d.url } });
});

chrome.idle.setDetectionInterval(60);
chrome.idle.onStateChanged.addListener((state) => send({ type: 'idle_state', payload: { state } }));

// ── Keepalive + lifecycle ─────────────────────────────────────────────────

chrome.alarms.create(KEEPALIVE, { periodInMinutes: 0.4 });
chrome.alarms.onAlarm.addListener((a) => {
  if (a.name !== KEEPALIVE) return;
  if (!ws || ws.readyState === WebSocket.CLOSED || ws.readyState === WebSocket.CLOSING) connect();
  else if (ws.readyState === WebSocket.OPEN) send({ type: 'ping' });
});

chrome.runtime.onStartup.addListener(connect);
chrome.runtime.onInstalled.addListener(() => {
  // Seed defaults so content scripts have the distraction set before the app
  // first connects (level stays off → no nudge until the app says otherwise).
  chrome.storage.local.set({ friction: { level: 'off', blockList: [] }, distractions: BUILTIN_DISTRACTIONS });
  connect();
  chrome.tabs.create({ url: chrome.runtime.getURL('walkthrough.html') });
});

// Let the walkthrough page show live connection status.
chrome.runtime.onMessage.addListener((msg, _sender, reply) => {
  if (msg === 'amdion-status') { reply({ connected }); return true; }
});

chrome.action.onClicked.addListener(() => {
  chrome.tabs.create({ url: chrome.runtime.getURL('walkthrough.html') });
});

// Connect whenever the worker spins up (any wake re-establishes the socket).
connect();
