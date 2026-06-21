// Amdion feature: Present — flip the active Chrome window to fullscreen (kills the
// tab strip + menu bar, the content-agnostic "wrap") and raise the distraction
// lock, reusing the Read-Mode wrap machinery. on:false restores both.

import { onBridge } from '../../core/bridge.js';
import { setReadingLock } from '../../core/block.js';
import { registerFeature, isEnabled } from '../../core/registry.js';

async function applyPresent(payload) {
  const on = payload.on !== false;
  chrome.windows.getLastFocused({}, (w) => {
    if (w && Number.isInteger(w.id)) {
      chrome.windows.update(w.id, { state: on ? 'fullscreen' : 'normal' }, () => void chrome.runtime.lastError);
    }
  });
  await setReadingLock(on);
}

// Gated on the feature flag (dormant by default): present_mode fullscreens the
// window from the worker (no content script needed), so it must check the flag.
onBridge('present_mode', (p) => { if (isEnabled('present')) applyPresent(p); });

// content/reader.js → the in-page "Present" offer: enter Present (fullscreen + the
// wrap) once a long task has settled. Same action present_mode drives. NOTE: the
// sole sender (features/read-mode/reader.js) is read-mode's content script, so this
// in-page offer only surfaces when read-mode is ALSO unlocked; present_mode over
// the bridge works with present alone.
chrome.runtime.onMessage.addListener((msg) => {
  if (msg && msg.type === 'amdion-present' && isEnabled('present')) applyPresent({ on: true });
});

registerFeature({ name: 'present' });
