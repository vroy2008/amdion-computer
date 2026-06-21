// Amdion feature: Present — flip the active Chrome window to fullscreen (kills the
// tab strip + menu bar, the content-agnostic "wrap") and raise the distraction
// lock, reusing the Read-Mode wrap machinery. on:false restores both.

import { onBridge } from '../../core/bridge.js';
import { setReadingLock } from '../../core/block.js';
import { registerFeature } from '../../core/registry.js';

async function applyPresent(payload) {
  const on = payload.on !== false;
  chrome.windows.getLastFocused({}, (w) => {
    if (w && Number.isInteger(w.id)) {
      chrome.windows.update(w.id, { state: on ? 'fullscreen' : 'normal' }, () => void chrome.runtime.lastError);
    }
  });
  await setReadingLock(on);
}

onBridge('present_mode', applyPresent);

// content/reader.js → the in-page "Present" offer: enter Present (fullscreen + the
// wrap) once a long task has settled. Same action present_mode drives.
chrome.runtime.onMessage.addListener((msg) => {
  if (msg && msg.type === 'amdion-present') applyPresent({ on: true });
});

registerFeature({ name: 'present' });
