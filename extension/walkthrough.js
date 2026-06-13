// Live "connected to Amdion" indicator for the walkthrough page. Asks the
// background worker for the current socket state and polls so the dot turns
// green the moment Amdion is running and the bridge is up.

const statusEl = document.getElementById('status');
const textEl = document.getElementById('status-text');

function paint(connected) {
  statusEl.classList.toggle('on', !!connected);
  textEl.textContent = connected
    ? 'Connected to Amdion'
    : 'Waiting for Amdion — make sure the app is running';
}

function poll() {
  try {
    chrome.runtime.sendMessage('amdion-status', (res) => {
      if (chrome.runtime.lastError) { paint(false); return; }
      paint(res && res.connected);
    });
  } catch (_) {
    paint(false);
  }
}

poll();
setInterval(poll, 2000);
