// Amdion feature: Notes/Capture — content-agnostic viewport screenshot capture
// (chrome.tabs.captureVisibleTab grabs rendered pixels, so it works even over
// Chrome's built-in PDF viewer, the one surface content scripts can't reach) plus
// relaying in-page highlights/typed notes. Both funnel to the app as note_captured
// on the reliable inbound bridge path.

import { send, onBridge } from '../../core/bridge.js';
import { registerFeature, isEnabled } from '../../core/registry.js';

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

function flashBadge() {
  try {
    chrome.action.setBadgeBackgroundColor({ color: '#2480ba' });
    chrome.action.setBadgeText({ text: '✓' });
    setTimeout(() => { try { chrome.action.setBadgeText({ text: '' }); } catch (_) {} }, 1200);
  } catch (_) {}
}

// Proactive capture is gated on the feature flag (dormant by default): the app
// push and the ⌃⇧C hotkey both no-op until Notes is unlocked.
onBridge('capture_tab', () => { if (isEnabled('notes')) captureActiveTab(); });

// The ⌃⇧C hotkey fires in the worker; capture the active tab.
chrome.commands.onCommand.addListener((command) => {
  if (command === 'capture-tab' && isEnabled('notes')) captureActiveTab();
});

// amdion-capture → app (note_captured). KEPT ALWAYS-ON even when the Notes feature
// is dormant: the core nudge's "Park it" (core/nudge.js) relies on this relay to
// file a page, and the app persists it (bridge_ws.rs → insert_note). The proactive
// capture surface (the ⌃⇧C hotkey above + features/notes/capture.js) is what's
// gated; this thin inbound relay is core Park-it plumbing.
chrome.runtime.onMessage.addListener((msg) => {
  if (msg && msg.type === 'amdion-capture') {
    send({ type: 'note_captured', payload: msg.payload || {} });
    flashBadge();
  }
});

registerFeature({
  name: 'notes',
  contentScripts: [
    { id: 'notes-capture', matches: ['<all_urls>'], js: ['features/notes/capture.js'], runAt: 'document_idle' },
  ],
});
