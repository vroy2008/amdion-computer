// Amdion Soft-mode nudge: a calm, dismissable banner shown when you land on a
// distraction domain while friction is "soft". Reads config from chrome.storage
// (written by the background worker) and re-evaluates when it changes. Rendered
// in a shadow root so the host page's CSS can't touch it.

(() => {
  const HOST = location.hostname.replace(/^www\./, '');
  let dismissed = false; // per page load
  let mount = null;

  const onDistraction = (domains) =>
    (domains || []).some((d) => HOST === d || HOST.endsWith('.' + d));

  // Connection / intentional surfaces we must never nudge, even on a distraction
  // domain — DMs and messaging are the whole point of being "allowed" here, so
  // nagging them is exactly the false-positive that erodes trust. Keyed by
  // registrable domain; values are path prefixes. (Landing-time only: a client-
  // side route change after load isn't re-checked here — that's Phase-2
  // behavioral sensing, not this CSS-cheap landing guard.)
  const PROTECTED_PATHS = {
    'instagram.com': ['/direct'],
    'x.com': ['/messages'],
    'twitter.com': ['/messages'],
    'facebook.com': ['/messages'],
    'linkedin.com': ['/messaging'],
    'reddit.com': ['/message', '/chat'],
    'tiktok.com': ['/messages'],
  };
  const isProtectedPath = () => {
    const path = location.pathname.toLowerCase();
    const key = Object.keys(PROTECTED_PATHS).find((d) => HOST === d || HOST.endsWith('.' + d));
    if (!key) return false;
    return PROTECTED_PATHS[key].some((p) => path === p || path.startsWith(p + '/'));
  };

  function remove() {
    if (mount) { mount.remove(); mount = null; }
  }

  // "Park it": the calm exit from a distraction — file this page (url + title)
  // to Amdion Notes over the same relay content/capture.js uses, then take the
  // user back. Fenced per the Defend guardrail: a write-and-forget, never a
  // queue or a badge.
  function park() {
    try {
      chrome.runtime.sendMessage(
        { type: 'amdion-capture', payload: { kind: 'link', source: 'web', url: location.href, title: document.title } },
        () => void chrome.runtime.lastError
      );
    } catch (_) {}
  }

  function show() {
    if (mount) return;
    mount = document.createElement('div');
    mount.id = 'amdion-nudge-host';
    mount.style.cssText = 'all: initial; position: fixed; top: 16px; left: 0; right: 0; z-index: 2147483647; display: flex; justify-content: center; pointer-events: none;';
    const shadow = mount.attachShadow({ mode: 'open' });
    shadow.innerHTML = `
      <style>
        .card {
          pointer-events: auto;
          font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
          display: flex; align-items: center; gap: 14px;
          background: rgba(18,18,18,0.92); color: #f5f5f5;
          border: 1px solid rgba(255,255,255,0.14); border-radius: 14px;
          padding: 12px 14px 12px 18px; max-width: 460px;
          box-shadow: 0 12px 40px rgba(0,0,0,0.5); backdrop-filter: blur(12px);
          animation: drop .28s cubic-bezier(.2,.7,.3,1) both;
        }
        @keyframes drop { from { opacity: 0; transform: translateY(-12px); } to { opacity: 1; transform: none; } }
        .mark { font-size: 10px; letter-spacing: .22em; color: #2480ba; font-weight: 600; }
        .txt { font-size: 13.5px; line-height: 1.45; flex: 1; }
        .txt b { font-weight: 600; }
        .btns { display: flex; gap: 6px; }
        button { font: inherit; font-size: 12px; cursor: pointer; border-radius: 8px; padding: 6px 11px; white-space: nowrap; }
        .leave { background: #2480ba; border: 1px solid #2480ba; color: #fff; }
        .leave:hover { background: #2c93d4; }
        .park { background: transparent; border: 1px solid rgba(255,255,255,0.18); color: #cfcfcf; }
        .park:hover { background: rgba(255,255,255,0.08); }
        .stay { background: transparent; border: 1px solid rgba(255,255,255,0.18); color: #cfcfcf; }
        .stay:hover { background: rgba(255,255,255,0.08); }
      </style>
      <div class="card" role="status">
        <div>
          <div class="mark">AMDION</div>
        </div>
        <div class="txt">You opened <b>${HOST}</b>. Is this where you meant to be?</div>
        <div class="btns">
          <button class="leave" data-act="leave">Take me back</button>
          <button class="park" data-act="park" title="Save this page to Amdion Notes and go back">Park it</button>
          <button class="stay" data-act="stay">Stay</button>
        </div>
      </div>`;
    const goBack = () => { if (history.length > 1) history.back(); else location.assign('about:blank'); };
    shadow.querySelector('[data-act="stay"]').onclick = () => { dismissed = true; remove(); };
    shadow.querySelector('[data-act="leave"]').onclick = () => { dismissed = true; goBack(); };
    shadow.querySelector('[data-act="park"]').onclick = () => { dismissed = true; park(); goBack(); };
    (document.body || document.documentElement).appendChild(mount);
  }

  function refresh() {
    chrome.storage.local.get(['friction', 'distractions'], (r) => {
      const level = (r.friction && r.friction.level) || 'off';
      if (level === 'soft' && !dismissed && onDistraction(r.distractions) && !isProtectedPath()) show();
      else remove();
    });
  }

  refresh();
  chrome.storage.onChanged.addListener((changes, area) => {
    if (area === 'local' && (changes.friction || changes.distractions)) refresh();
  });
})();
