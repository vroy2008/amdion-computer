// Amdion nudge — a calm, dismissable card shown when attention may have drifted.
// Reads config from chrome.storage (written by the background worker) and reacts
// to changes. Rendered in a shadow root so the host page's CSS can't touch it.
//
// One card, several triggers (docs/REORIENTATION.md §5):
//   • landing      — you land on a distraction while friction is "soft" (the
//                     original behavior; friction-gated).
//   • over-scroll  — you've actively scrolled a feed well past the point of
//                     value (sensed here; reshape-gated, friction-independent).
//   • redirect     — an ad/redirect chain dropped you on a distraction
//                     (detected in background.js → message; reshape-gated).
//   • idle-return  — you came back from a break onto an open distraction
//                     (detected in background.js → message; reshape-gated).
//   • drift        — a YouTube watch→watch rabbit hole (content/ytdrift.js calls
//                     globalThis.__amdionNudge.show).
//
// Behavioral triggers are gated on the per-site *reshape* switch, not the
// friction level — so a site can be calmed even in Off mode (§9). When an intent
// is set for the session the copy adapts ("You're here for X — is HOST part of
// that?"). At most one nudge per page load, so we never nag.

(() => {
  const EXT = typeof chrome !== 'undefined' && chrome.runtime && chrome.runtime.id ? chrome : null;
  if (!EXT) return;

  const HOST = location.hostname.replace(/^www\./, '');
  const BUILTIN_DISTRACTIONS = [
    'youtube.com', 'twitter.com', 'x.com', 'facebook.com', 'instagram.com',
    'reddit.com', 'tiktok.com', 'netflix.com', 'twitch.tv',
  ];
  let dismissed = false; // one nudge per page load, once dealt with
  let mount = null;
  let mountReason = null;
  let intent = null;

  const onList = (host, list) =>
    (list || []).some((d) => host === d || host.endsWith('.' + d));
  const onDistraction = (domains) => onList(HOST, domains);

  // Connection / intentional surfaces we must never nudge, even on a distraction
  // domain — DMs and messaging are the whole point of being "allowed" here, so
  // nagging them is exactly the false-positive that erodes trust. (Landing-time
  // path read; a client-side SPA route change after load isn't re-checked.)
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

  // Is this host reshaped right now? Mirrors content/reshape.js. We prefer the
  // live shared state it publishes, falling back to storage when a behavioral
  // trigger somehow fires before reshape.js resolved.
  function reshapeOn(cb) {
    const live = globalThis.__amdionReshape;
    if (live && typeof live.on === 'boolean' && live.host === HOST) return cb(live.on);
    EXT.storage.local.get(['reshape', 'distractions'], (r) => {
      const reshape = r.reshape || { enabled: true, disabledSites: [] };
      const distractions = r.distractions || BUILTIN_DISTRACTIONS;
      if (reshape.enabled === false || onList(HOST, reshape.disabledSites)) return cb(false);
      cb(onDistraction(distractions));
    });
  }

  function escapeHtml(s) {
    return String(s).replace(/[&<>"']/g, (c) =>
      ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  }

  // "Park it": the calm exit — file this page (url + title) to Amdion Notes over
  // the same relay content/capture.js uses, then take the user back. Fenced per
  // the Defend guardrail: a write-and-forget, never a queue or a badge.
  function park() {
    try {
      chrome.runtime.sendMessage(
        { type: 'amdion-capture', payload: { kind: 'link', source: 'web', url: location.href, title: document.title } },
        () => void chrome.runtime.lastError
      );
    } catch (_) {}
  }

  const goBack = () => { if (history.length > 1) history.back(); else location.assign('about:blank'); };

  // ── Copy + buttons per trigger ────────────────────────────────────────────
  function copyFor(reason) {
    const h = `<b>${escapeHtml(HOST)}</b>`;
    const i = intent ? `<b>${escapeHtml(intent)}</b>` : null;
    switch (reason) {
      case 'overscroll':
        return i ? `You're here for ${i} — still finding it on ${h}?`
                 : `Still finding what you came for on ${h}?`;
      case 'redirect':
        return `You were redirected to ${h}. Hold it for later?`;
      case 'idle-return':
        return i ? `Back on ${h}. Still on ${i}?`
                 : `Back on ${h}. Pick up where you meant to?`;
      case 'drift':
        return i ? `That's a few in a row. Still on ${i}?`
                 : `That's a few videos in a row. Still on track?`;
      case 'landing':
      default:
        return i ? `You're here for ${i} — is ${h} part of that?`
                 : `You opened ${h}. Is this where you meant to be?`;
    }
  }

  // act: 'leave' (go back), 'park' (save + go back), 'stay' (dismiss). The first
  // button is the primary (accent) action.
  function buttonsFor(reason) {
    switch (reason) {
      case 'redirect':
        return [
          { label: 'Hold for later', act: 'park', primary: true, title: 'Save this page to Amdion Notes and go back' },
          { label: 'Take me back', act: 'leave' },
          { label: 'Stay', act: 'stay' },
        ];
      case 'drift':
        return [
          { label: 'Step away', act: 'leave', primary: true },
          { label: 'Keep going', act: 'stay' },
        ];
      default:
        return [
          { label: 'Take me back', act: 'leave', primary: true },
          { label: 'Park it', act: 'park', title: 'Save this page to Amdion Notes and go back' },
          { label: 'Stay', act: 'stay' },
        ];
    }
  }

  function show(reason) {
    if (mount || dismissed) return;
    mountReason = reason;
    mount = document.createElement('div');
    mount.id = 'amdion-nudge-host';
    mount.style.cssText = 'all: initial; position: fixed; top: 16px; left: 0; right: 0; z-index: 2147483647; display: flex; justify-content: center; pointer-events: none;';
    const shadow = mount.attachShadow({ mode: 'open' });
    const btns = buttonsFor(reason);
    const btnHtml = btns.map((b, idx) =>
      `<button class="${b.primary ? 'primary' : 'ghost'}" data-act="${b.act}"${b.title ? ` title="${escapeHtml(b.title)}"` : ''}>${escapeHtml(b.label)}</button>`
    ).join('');
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
        .primary { background: #2480ba; border: 1px solid #2480ba; color: #fff; }
        .primary:hover { background: #2c93d4; }
        .ghost { background: transparent; border: 1px solid rgba(255,255,255,0.18); color: #cfcfcf; }
        .ghost:hover { background: rgba(255,255,255,0.08); }
      </style>
      <div class="card" role="status">
        <div>
          <div class="mark">AMDION</div>
        </div>
        <div class="txt">${copyFor(reason)}</div>
        <div class="btns">${btnHtml}</div>
      </div>`;
    const act = (kind) => {
      dismissed = true;
      if (kind === 'leave') goBack();
      else if (kind === 'park') { park(); goBack(); }
      else remove(); // stay
    };
    shadow.querySelectorAll('button').forEach((b) => { b.onclick = () => act(b.dataset.act); });
    (document.body || document.documentElement).appendChild(mount);
  }

  function remove() {
    if (mount) { mount.remove(); mount = null; mountReason = null; }
  }

  // The landing nudge — friction-gated (Soft), fires on a distraction landing.
  // Reactive to config: show when wanted; retract ONLY a landing card when it's
  // no longer wanted (e.g. friction dropped to Off) — never a behavioral card.
  function refresh() {
    EXT.storage.local.get(['friction', 'distractions', 'intent'], (r) => {
      intent = (r.intent && String(r.intent).trim()) || null;
      const level = (r.friction && r.friction.level) || 'off';
      const wantLanding = level === 'soft' && onDistraction(r.distractions) && !isProtectedPath();
      if (wantLanding && !dismissed && !mount) show('landing');
      else if (!wantLanding && mountReason === 'landing') remove();
    });
  }

  // A behavioral trigger (over-scroll / redirect / idle-return / drift). Honors
  // the protected-path guard and the one-per-load rule; reshape/distraction
  // gating is the caller's job (background.js for redirect/idle; reshapeOn here
  // for over-scroll; ytdrift.js for drift).
  function fireBehavioral(reason) {
    if (mount || dismissed || isProtectedPath()) return;
    // A calm long read isn't a drift to interrupt — suppress every behavioral
    // trigger on an article-like page (over-scroll already pre-checks; this also
    // covers the background-driven redirect / idle-return signals and drift).
    const readerable = typeof isProbablyReaderable !== 'undefined' && (() => {
      try { return isProbablyReaderable(document); } catch (_) { return false; }
    })();
    if (readerable) return;
    show(reason);
  }

  // ── Over-scroll sensing ─────────────────────────────────────────────────
  // Active-scroll *time* is the honest signal (raw screen count nags a genuine
  // long read). We accumulate time only between scroll events close in time;
  // gaps reset nothing but stop accruing. Generous default; once per page load;
  // suppressed on reader-able pages and protected paths.
  const OVERSCROLL_MS = 70000; // ~70s of active scrolling past the fold
  let lastScrollTs = 0;
  let activeScrollMs = 0;
  let overscrollFired = false;
  function onScroll() {
    if (overscrollFired || dismissed) return;
    const now = Date.now();
    if (lastScrollTs && now - lastScrollTs < 1500) activeScrollMs += now - lastScrollTs;
    lastScrollTs = now;
    if (activeScrollMs < OVERSCROLL_MS) return;
    // Suppress on article-like pages (the reader group's readerable check shares
    // our isolated world); never on protected paths. These are page-load-stable,
    // so latch and stop checking.
    const readerable = typeof isProbablyReaderable !== 'undefined' && (() => {
      try { return isProbablyReaderable(document); } catch (_) { return false; }
    })();
    if (readerable || isProtectedPath()) { overscrollFired = true; return; }
    // Reshape can be flipped on mid-session; only consume the trigger once we
    // actually fire, so enabling reshape on an open tab still lets it nudge.
    reshapeOn((on) => { if (on) { overscrollFired = true; fireBehavioral('overscroll'); } });
  }

  // ── Wiring ────────────────────────────────────────────────────────────────
  refresh();
  EXT.storage.onChanged.addListener((changes, area) => {
    if (area === 'local' && (changes.friction || changes.distractions || changes.intent)) refresh();
  });
  window.addEventListener('scroll', onScroll, { passive: true });
  EXT.runtime.onMessage.addListener((msg) => {
    if (!msg || msg.type !== 'amdion-nudge') return;
    fireBehavioral(msg.reason); // background already checked reshape + distraction
  });

  // Shared API so other content scripts (e.g. content/ytdrift.js) can raise the
  // same card without duplicating its markup.
  globalThis.__amdionNudge = { show: (opts) => fireBehavioral((opts && opts.reason) || 'landing'), hide: remove };
})();
