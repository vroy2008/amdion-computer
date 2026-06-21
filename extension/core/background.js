// Amdion companion — MV3 service worker entry (ES module).
//
// Intentionally thin: it imports the core (bridge, block) and every bonus feature
// module (which self-register via core/registry.js), wires the always-on activity
// reporting + a few core App→Ext handlers, then opens the bridge. Feature
// internals live under features/<name>/ and are never referenced here beyond this
// import manifest — so a feature can be edited in isolation without touching core.

import { connect, send, isConnected, onBridge } from './bridge.js';
import { BUILTIN_DISTRACTIONS, refreshBlocking } from './block.js';
import { dispatch, featureDefaults, setEnabledMap } from './registry.js';

// Feature modules — importing them runs their top-level self-registration. Each
// is acyclic: features import from core/, never the reverse.
import '../features/reshape/background.js';
import '../features/read-mode/background.js';
import '../features/notes/background.js';
import '../features/present/background.js';

// ── Feature enable-gate: mirror chrome.storage.local 'features' → registry ────
// Absent ⇒ every feature enabled (today's default). Live-updates if the app
// later flips one dormant. Not a top-level await, so listeners below still
// register synchronously on worker startup (MV3 wake requirement).
chrome.storage.local.get(['features']).then((r) => setEnabledMap(r.features || {}));
chrome.storage.onChanged.addListener((changes, area) => {
  if (area === 'local' && changes.features) setEnabledMap(changes.features.newValue || {});
});

// ── Core App→Ext handlers: tab control + intent ──────────────────────────────

onBridge('open_tab', (payload) => { if (payload && payload.url) chrome.tabs.create({ url: payload.url }); });
onBridge('focus_tab', (payload) => focusTab(payload && payload.tabId));
onBridge('close_tab', (payload) => closeTab(payload && payload.tabId));
onBridge('intent', applyIntent);

// `tabId` from the app may be Amdion's internal id (not a Chrome tab id) until the
// id map exists — coerce and bail unless it's a real integer id.
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

// Intent rides over the bridge and is mirrored to storage where the nudge copy
// reads it. (Driving the friction mode from intent lands in the modes step.)
function applyIntent(payload) {
  const raw = payload && typeof payload.intent === 'string' ? payload.intent.trim() : '';
  chrome.storage.local.set({ intent: raw || null });
}

// ── Activity reporting (track — always on, all modes) ─────────────────────────
// Core streams tab/idle/nav events to the app; feature hooks (reshape's
// redirect/idle nudges) ride along via the registry dispatch.

const tabInfo = (t) => ({ tabId: t.id, url: t.url, title: t.title, windowId: t.windowId });

chrome.tabs.onCreated.addListener((t) => send({ type: 'tab_opened', payload: tabInfo(t) }));
chrome.tabs.onRemoved.addListener((tabId) => send({ type: 'tab_closed', payload: { tabId } }));
chrome.tabs.onActivated.addListener(({ tabId }) => {
  chrome.tabs.get(tabId, (t) => { if (!chrome.runtime.lastError && t) send({ type: 'tab_activated', payload: tabInfo(t) }); });
});
chrome.webNavigation.onCommitted.addListener((d) => {
  if (d.frameId !== 0) return;
  send({ type: 'tab_navigated', payload: { tabId: d.tabId, url: d.url } });
  dispatch('onNavCommitted', d); // reshape: redirect/ad-chase → hold-for-later
});

let lastIdleState = 'active';
chrome.idle.setDetectionInterval(60);
chrome.idle.onStateChanged.addListener((state) => {
  send({ type: 'idle_state', payload: { state } });
  // Returning to the keyboard after a break, onto an open distraction → re-anchor.
  if (state === 'active' && (lastIdleState === 'idle' || lastIdleState === 'locked')) {
    dispatch('onActiveFromIdle'); // reshape: idle-return re-anchor
  }
  lastIdleState = state;
});

// ── Lifecycle ────────────────────────────────────────────────────────────────

chrome.runtime.onStartup.addListener(() => {
  // A fresh browser session can't have a reader mid-read, so clear any wrap a
  // crash/quit during a previous read left set — otherwise it would keep blocking
  // with no reader open. (Worker recycles don't fire onStartup, so a genuinely
  // ongoing read keeps its lock.)
  chrome.storage.local.set({ readingLock: false }).then(refreshBlocking);
  connect();
});

chrome.runtime.onInstalled.addListener(() => {
  // Seed core defaults + each feature's defaults so content scripts have what they
  // need before the app first connects (level stays off → no nudge until the app
  // says otherwise).
  chrome.storage.local.set({
    friction: { level: 'off', blockList: [] },
    distractions: BUILTIN_DISTRACTIONS,
    readingLock: false,
    intent: null,
    ...featureDefaults(),
  });
  connect();
  chrome.tabs.create({ url: chrome.runtime.getURL('walkthrough.html') });
});

// Let the walkthrough page show live connection status.
chrome.runtime.onMessage.addListener((msg, _sender, reply) => {
  if (msg === 'amdion-status') { reply({ connected: isConnected() }); return true; }
});

chrome.action.onClicked.addListener(() => {
  chrome.tabs.create({ url: chrome.runtime.getURL('walkthrough.html') });
});

// Connect once every import above has registered its handlers and hooks.
connect();
