// Amdion feature: Read Mode — enter/exit the in-page reader and manage the
// distraction "wrap" around a read. The reader itself lives in the content script
// (features/read-mode/reader.js); this routes the three triggers (⌃⇧R hotkey, the
// app's read_mode, the in-page pill) to the right tab and raises/lowers the wrap
// on read boundaries. The lock half runs locally so it works even bridge-down.

import { send, onBridge } from '../../core/bridge.js';
import { setReadingLock } from '../../core/block.js';
import { registerFeature } from '../../core/registry.js';

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

// App→Ext: mirror reading prefs to storage where reader.js watches + live-applies
// (theme, size, pill, …).
function applyReadPrefs(payload) {
  const reading = payload || {};
  chrome.storage.local.set({ reading });
  // Turning the wrap off (in the panel) releases an in-progress lock at once.
  if (reading.lockTabs === false) setReadingLock(false);
}

// Raise the wrap on read start (unless opted out via lockTabs); always lower on
// read end. A refresh recomputes from the base level, so lowering is the restore.
async function applyReadingWrap(started) {
  if (!started) return setReadingLock(false);
  const { reading } = await chrome.storage.local.get(['reading']);
  if (reading && reading.lockTabs === false) return; // default on; explicit opt-out only
  await setReadingLock(true);
}

onBridge('read_mode', applyReadMode);
onBridge('read_prefs', applyReadPrefs);

// The ⌃⇧R hotkey fires in the worker; relay it to whatever tab is in front.
chrome.commands.onCommand.addListener((command) => {
  if (command === 'toggle-read-mode') withActiveTab((id) => sendToTab(id, 'amdion-read-enter'));
});

// content/reader.js → app + local wrap: forward read_started/read_ended over the
// bridge (the app logs reading time + runs an optional Focus Shortcut) and apply
// the lock-distractions half of the wrap right here.
chrome.runtime.onMessage.addListener((msg) => {
  if (msg && msg.type === 'amdion-read-event') {
    const started = msg.event === 'started';
    send({ type: started ? 'read_started' : 'read_ended', payload: msg.payload || {} });
    applyReadingWrap(started);
  }
});

registerFeature({ name: 'read-mode' });
