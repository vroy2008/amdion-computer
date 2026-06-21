// Amdion companion — MV3 service worker entry (ES module).
//
// Intentionally thin: it imports the core (bridge, block) and every bonus feature
// module (which self-register via core/registry.js), wires the always-on activity
// reporting + a few core App→Ext handlers, then opens the bridge. Feature
// internals live under features/<name>/ and are never referenced here beyond this
// import manifest — so a feature can be edited in isolation without touching core.

import { connect, send, isConnected, onBridge } from './bridge.js';
import { BUILTIN_DISTRACTIONS, refreshBlocking, applyManualMode } from './block.js';
import { dispatch, featureDefaults, setEnabledMap, enabledContentScripts } from './registry.js';

// Feature modules — importing them runs their top-level self-registration. Each
// is acyclic: features import from core/, never the reverse.
import '../features/reshape/background.js';
import '../features/read-mode/background.js';
import '../features/notes/background.js';
import '../features/present/background.js';

// ── Feature enable-gate: mirror chrome.storage.local 'features' → registry ────
// Absent ⇒ every bonus feature DORMANT (V1 default). Live-updates if the app
// later unlocks one. Not a top-level await, so listeners below still register
// synchronously on worker startup (MV3 wake requirement).
chrome.storage.local.get(['features']).then((r) => { setEnabledMap(r.features || {}); syncContentScripts(); });
chrome.storage.onChanged.addListener((changes, area) => {
  if (area === 'local' && changes.features) { setEnabledMap(changes.features.newValue || {}); syncContentScripts(); }
});

// Register only the content scripts the ENABLED features want, reconciled against
// the live registration set so unlocking/locking a feature adds/removes its
// scripts. Dormant features inject nothing — no per-page cost. chrome.scripting
// registrations persist across sessions AND extension updates, so we reconcile by
// CONTENT, not just id: an id whose matches/js/css/runAt changed is dropped and
// re-added (else a shipped spec change wouldn't take effect), and a since-locked
// feature's scripts are dropped — self-healing on every worker startup. Calls are
// serialized so rapid unlock/lock toggles can't interleave onto the wrong state.
let csSyncChain = Promise.resolve();
function syncContentScripts() {
  csSyncChain = csSyncChain.then(runContentScriptSync).catch((e) => console.warn('[amdion] content-script sync failed:', e));
  return csSyncChain;
}
const csSig = (s) => JSON.stringify({ m: s.matches || [], js: s.js || [], css: s.css || [], r: s.runAt || 'document_idle' });
async function runContentScriptSync() {
  if (!chrome.scripting || !chrome.scripting.registerContentScripts) return;
  const want = enabledContentScripts();
  const wantById = new Map(want.map((s) => [s.id, s]));
  let existing = [];
  try { existing = await chrome.scripting.getRegisteredContentScripts(); } catch (_) {}
  const existById = new Map(existing.map((s) => [s.id, s]));
  // Drop ids no longer wanted OR whose spec changed; (re-)add new ids + changed specs.
  const toRemove = existing.filter((s) => !wantById.has(s.id) || csSig(s) !== csSig(wantById.get(s.id))).map((s) => s.id);
  const toAdd = want.filter((s) => !existById.has(s.id) || csSig(existById.get(s.id)) !== csSig(s));
  try { if (toRemove.length) await chrome.scripting.unregisterContentScripts({ ids: toRemove }); }
  catch (e) { console.warn('[amdion] unregister content scripts failed:', e); }
  try { if (toAdd.length) await chrome.scripting.registerContentScripts(toAdd); }
  catch (e) { console.warn('[amdion] register content scripts failed:', e); }
}

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
  // need before the app first connects. Default mode = `soft` (Nudge): Defend is
  // gently-on out of the box, before any app connection (V1.md §3.2). Block stays
  // opt-in (intent = Deep work, or a manual toggle). Tracking runs in every mode.
  chrome.storage.local.set({
    friction: { level: 'soft', blockList: [] },
    distractions: BUILTIN_DISTRACTIONS,
    readingLock: false,
    intent: null,
    ...featureDefaults(),
  });
  refreshBlocking();
  connect();
  chrome.tabs.create({ url: chrome.runtime.getURL('walkthrough.html') });
});

// Let the walkthrough page + the action popup read live connection status, and
// let the popup set the mode (a manual, session-scoped override). The action
// itself opens the popup (manifest `action.default_popup`), so there's no
// onClicked handler — the popup links to the walkthrough for first-time setup.
chrome.runtime.onMessage.addListener((msg, _sender, reply) => {
  if (msg === 'amdion-status') { reply({ connected: isConnected() }); return true; }
  if (msg && msg.type === 'amdion-set-mode') {
    applyManualMode(msg.level).then(() => reply({ ok: true }));
    return true; // keep the channel open for the async reply
  }
});

// Connect once every import above has registered its handlers and hooks.
connect();
