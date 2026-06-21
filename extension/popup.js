// Amdion action popup — the manual Off / Nudge / Block toggle.
//
// The popup is a thin remote: it reflects the effective mode (chrome.storage.local
// `friction.level`) and, on a click, asks the service worker to set it as a
// MANUAL, session-scoped override (so all the blocking/self-heal logic stays in
// one place — core/block.js — instead of being duplicated here).

const DESC = {
  off: 'Just tracking your activity — no nudges, no blocking.',
  soft: 'A gentle nudge card when you land on a distraction.',
  lockin: 'Your distraction sites are blocked in Chrome.',
};

const segEl = document.getElementById('seg-mode');
const descEl = document.getElementById('desc');
const statusEl = document.getElementById('status');
const statusText = document.getElementById('status-text');

let connected = false;
let modeSource = 'intent';

function render(level) {
  const mode = ['off', 'soft', 'lockin'].includes(level) ? level : 'soft';
  segEl.querySelectorAll('button').forEach((b) => b.classList.toggle('active', b.dataset.v === mode));
  descEl.innerHTML = DESC[mode];
  statusEl.classList.toggle('on', connected);
  statusText.textContent = connected
    ? (modeSource === 'manual' ? 'Connected · manual, this session' : 'Connected · follows your intent')
    : 'Not connected — using your choice';
}

function load() {
  chrome.storage.local.get(['friction', 'modeSource'], (r) => {
    modeSource = r.modeSource || 'intent';
    render((r.friction && r.friction.level) || 'soft');
  });
  // Ask the worker whether the desktop app is connected (wakes it if asleep).
  chrome.runtime.sendMessage('amdion-status', (resp) => {
    void chrome.runtime.lastError;
    connected = !!(resp && resp.connected);
    statusEl.classList.toggle('on', connected);
    // Re-render status text now that we know connection + source.
    chrome.storage.local.get(['friction'], (r) => render((r.friction && r.friction.level) || 'soft'));
  });
}

segEl.querySelectorAll('button').forEach((b) => {
  b.onclick = () => {
    const level = b.dataset.v;
    modeSource = 'manual'; // a click is always a manual override
    render(level); // optimistic
    chrome.runtime.sendMessage({ type: 'amdion-set-mode', level }, () => void chrome.runtime.lastError);
  };
});

document.getElementById('help-link').onclick = () => {
  chrome.tabs.create({ url: chrome.runtime.getURL('walkthrough.html') });
  window.close();
};

load();
