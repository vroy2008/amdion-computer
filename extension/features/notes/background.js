// Amdion feature: Notes/Capture — content-agnostic viewport screenshot capture
// (chrome.tabs.captureVisibleTab grabs rendered pixels, so it works even over
// Chrome's built-in PDF viewer, the one surface content scripts can't reach) plus
// relaying in-page highlights/typed notes. Both funnel to the app as note_captured
// on the reliable inbound bridge path.

import { send, onBridge } from '../../core/bridge.js';
import { registerFeature } from '../../core/registry.js';

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

onBridge('capture_tab', captureActiveTab);

// The ⌃⇧C hotkey fires in the worker; capture the active tab.
chrome.commands.onCommand.addListener((command) => {
  if (command === 'capture-tab') captureActiveTab();
});

// content/capture.js → app: a highlight (selected quote) or typed note from a
// normal web page. Relay it over the bridge as note_captured.
chrome.runtime.onMessage.addListener((msg) => {
  if (msg && msg.type === 'amdion-capture') {
    send({ type: 'note_captured', payload: msg.payload || {} });
    flashBadge();
  }
});

registerFeature({ name: 'notes' });
