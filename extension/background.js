// Amdion companion — MV3 service worker.
//
// Connects to the always-running Amdion app over the localhost WebSocket bridge,
// reports tab/idle activity, applies the user's friction (Soft nudge / Lock-In
// block), and opens/focuses tabs on command. MV3 service workers are ephemeral,
// so a `chrome.alarms` heartbeat wakes us to reconnect and to keep a live socket
// warm (an open socket alone does NOT keep an MV3 worker alive).

// The app binds the first free port in ITS range; we scan dev ports FIRST so a
// running `tauri dev` instance always wins over the installed release app on the
// same machine (they use SEPARATE ranges — see src-tauri/src/bridge_ws.rs). With
// only release up, the cursor walks past the dead dev ports and lands on release
// (normal end-user behavior). DEV_PORTS/REL_PORTS must stay in sync with the
// debug_assertions split in bridge_ws.rs.
const DEV_PORTS = Array.from({ length: 11 }, (_, i) => 17883 + i); // dev:     17883–17893
const REL_PORTS = Array.from({ length: 11 }, (_, i) => 17872 + i); // release: 17872–17882
const PORTS = [...DEV_PORTS, ...REL_PORTS];
const DEV_PORT_SET = new Set(DEV_PORTS);

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
let connectedPort = null; // the port the live socket is on (tells dev vs release range)
let migrateProbe = null;  // throwaway socket used to detect a dev instance to migrate to

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

// While connected to a RELEASE port, periodically check whether a dev instance
// has since come up; if so, migrate to it — so `release running → run tauri dev`
// switches the extension to the new build with no manual reload. Non-destructive:
// we only tear down the live release socket once a dev port actually accepts a
// connection, never on a failed probe. Bounded: at most one in-flight probe,
// ≤11 dev ports, once per keepalive tick, and an instant no-op once on dev.
function maybeMigrateToDev() {
  if (!ws || ws.readyState !== WebSocket.OPEN) return;                  // only from a live socket
  if (connectedPort == null || DEV_PORT_SET.has(connectedPort)) return; // already on dev
  if (migrateProbe) return;                                             // a probe is already in flight

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
    case 'capture_tab': captureActiveTab(); break;
    case 'present_mode': applyPresent(msg.payload || {}); break;
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
  await refreshBlocking();
}

// Recompute Lock-In blocking from the *effective* level: the user's base
// friction, escalated to Lock-In while a read is open ("the wrap"). Driving
// every rule rebuild through one function — over state that lives in storage,
// not memory — means snapshot/restore is implicit (the base level IS the
// snapshot) and self-healing: an MV3 worker restart, a reconnect, or any
// friction push recomputes the truth, so a read lock can never strand the user.
async function refreshBlocking() {
  const { friction, distractions, readingLock } = await chrome.storage.local.get([
    'friction', 'distractions', 'readingLock',
  ]);
  const base = (friction && friction.level) || 'off';
  const set = Array.isArray(distractions) ? distractions : BUILTIN_DISTRACTIONS;
  await rebuildBlockingRules(readingLock ? 'lockin' : base, set);
}

// Raise/lower "the wrap". Persisted to storage (survives a worker restart) and
// applied via the effective-level recompute above.
async function setReadingLock(on) {
  await chrome.storage.local.set({ readingLock: !!on });
  await refreshBlocking();
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
//   • the ⌃⇧R hotkey (chrome.commands `MacCtrl+Shift+R`, below),
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
  const reading = payload || {};
  chrome.storage.local.set({ reading });
  // Turning the wrap off (in the panel) releases an in-progress lock at once.
  if (reading.lockTabs === false) setReadingLock(false);
}

// The hotkey fires in the worker; relay it to whatever tab is in front.
chrome.commands.onCommand.addListener((command) => {
  if (command === 'toggle-read-mode') withActiveTab((id) => sendToTab(id, 'amdion-read-enter'));
  else if (command === 'capture-tab') captureActiveTab();
});

// ── Attention layer: Capture + Present ──────────────────────────────────────
//
// Capture is content-agnostic and permission-free: a viewport screenshot
// (chrome.tabs.captureVisibleTab) grabs rendered pixels, so it works even over
// Chrome's built-in PDF viewer — the one surface content scripts can't reach.
// Highlights/typed notes from normal pages arrive separately from content/
// capture.js as 'amdion-capture'. Both funnel to the app as `note_captured`,
// which the app persists on the reliable inbound bridge path.

function captureActiveTab() {
  chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
    const tab = tabs && tabs[0];
    if (!tab || !Number.isInteger(tab.windowId)) return;
    chrome.tabs.captureVisibleTab(tab.windowId, { format: 'png' }, (dataUrl) => {
      if (chrome.runtime.lastError || !dataUrl) return;
      const url = tab.url || '';
      const source = /\.pdf(\?|#|$)/i.test(url) || url.startsWith('file:') ? 'pdf' : 'web';
      send({ type: 'note_captured', payload: { kind: 'screenshot', source, url, title: tab.title || '', image: dataUrl } });
      flashBadge();
    });
  });
}

// Present: flip the active Chrome window to fullscreen (kills the tab strip +
// menu bar — the content-agnostic "wrap") and raise the distraction lock,
// reusing the Read-Mode wrap machinery. `on:false` restores both.
async function applyPresent(payload) {
  const on = payload.on !== false;
  chrome.windows.getLastFocused({}, (w) => {
    if (w && Number.isInteger(w.id)) {
      chrome.windows.update(w.id, { state: on ? 'fullscreen' : 'normal' }, () => void chrome.runtime.lastError);
    }
  });
  await setReadingLock(on);
}

function flashBadge() {
  try {
    chrome.action.setBadgeBackgroundColor({ color: '#2480ba' });
    chrome.action.setBadgeText({ text: '✓' });
    setTimeout(() => { try { chrome.action.setBadgeText({ text: '' }); } catch (_) {} }, 1200);
  } catch (_) {}
}

// content/capture.js → app: a highlight (selected quote) or typed note from a
// normal web page. We just relay it over the bridge as `note_captured`.
chrome.runtime.onMessage.addListener((msg) => {
  if (msg && msg.type === 'amdion-capture') {
    send({ type: 'note_captured', payload: msg.payload || {} });
    flashBadge();
  }
});

// content/reader.js → app + local wrap. We (a) forward read_started/read_ended
// over the bridge so the app can log reading time and run an optional Focus
// Shortcut, and (b) apply the lock-distractions half of the wrap right here, so
// it works even when the app/bridge is down (matching the reader itself).
chrome.runtime.onMessage.addListener((msg) => {
  if (msg && msg.type === 'amdion-read-event') {
    const started = msg.event === 'started';
    send({ type: started ? 'read_started' : 'read_ended', payload: msg.payload || {} });
    applyReadingWrap(started);
  }
});

// Raise the wrap on read start (unless the user opted out via lockTabs); always
// lower it on read end. A refresh recomputes from the base level, so lowering is
// the restore.
async function applyReadingWrap(started) {
  if (!started) return setReadingLock(false);
  const { reading } = await chrome.storage.local.get(['reading']);
  if (reading && reading.lockTabs === false) return; // default on; explicit opt-out only
  await setReadingLock(true);
}

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
  if (!ws || ws.readyState === WebSocket.CLOSED || ws.readyState === WebSocket.CLOSING) {
    connect();
  } else if (ws.readyState === WebSocket.OPEN) {
    send({ type: 'ping' });
    maybeMigrateToDev(); // cheap dev re-probe while parked on a release port
  }
});

chrome.runtime.onStartup.addListener(() => {
  // A fresh browser session can't have a reader mid-read, so clear any wrap a
  // crash/quit during a previous read left set — otherwise it would keep
  // blocking with no reader open. (Ephemeral worker recycles don't fire
  // onStartup, so a genuinely ongoing read keeps its lock.)
  chrome.storage.local.set({ readingLock: false }).then(refreshBlocking);
  connect();
});
chrome.runtime.onInstalled.addListener(() => {
  // Seed defaults so content scripts have the distraction set before the app
  // first connects (level stays off → no nudge until the app says otherwise).
  chrome.storage.local.set({ friction: { level: 'off', blockList: [] }, distractions: BUILTIN_DISTRACTIONS, readingLock: false });
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
