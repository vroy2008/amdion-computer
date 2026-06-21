// Amdion reshape — the nudge triggers that key off the per-site *reshape* switch
// (not the friction level), so a site can be calmed even in Off mode (§9). The
// card itself and the friction-gated landing trigger are core (core/nudge.js);
// these are the reshape-gated *behavioral* triggers, split out so core never
// depends on the reshape feature:
//
//   • over-scroll  — you've actively scrolled a feed well past the point of value
//                    (sensed here; reshape-gated, friction-independent).
//   • redirect     — an ad/redirect chain dropped you on a distraction (detected
//                    in features/reshape/background.js → runtime message).
//   • idle-return  — you came back from a break onto an open distraction (detected
//                    in the background → runtime message).
//
// (The YouTube watch→watch drift trigger is the same pattern in ytdrift.js.) All
// hand off to the shared card via window.__amdion.nudge.show({reason}); that API
// owns the protected-path / reader-able / one-per-load guards, so this file only
// decides *when* a reshape-gated trigger fires.

(() => {
  const EXT = typeof chrome !== 'undefined' && chrome.runtime && chrome.runtime.id ? chrome : null;
  if (!EXT) return;
  const NS = (window.__amdion = window.__amdion || {});

  const HOST = location.hostname.replace(/^www\./, '');
  const BUILTIN_DISTRACTIONS = [
    'youtube.com', 'twitter.com', 'x.com', 'facebook.com', 'instagram.com',
    'reddit.com', 'tiktok.com', 'netflix.com', 'twitch.tv',
  ];
  const onList = (host, list) =>
    (list || []).some((d) => host === d || host.endsWith('.' + d));
  const onDistraction = (domains) => onList(HOST, domains);

  // Raise the shared card. A missing API (core/nudge.js didn't load — same EXT
  // guard, so only if something is very wrong) just no-ops.
  const raise = (reason) => { if (NS.nudge) NS.nudge.show({ reason }); };

  // Is this host reshaped right now? Prefer reshape.js's live shared state; fall
  // back to storage when a trigger somehow fires before reshape.js resolved.
  // (Mirrors the gate in features/reshape/reshape.js.)
  function reshapeOn(cb) {
    const live = NS.reshape;
    if (live && typeof live.on === 'boolean' && live.host === HOST) return cb(live.on);
    EXT.storage.local.get(['reshape', 'distractions'], (r) => {
      const reshape = r.reshape || { enabled: true, disabledSites: [] };
      const distractions = r.distractions || BUILTIN_DISTRACTIONS;
      if (reshape.enabled === false || onList(HOST, reshape.disabledSites)) return cb(false);
      cb(onDistraction(distractions));
    });
  }

  // ── Over-scroll sensing ─────────────────────────────────────────────────────
  // Active-scroll *time* is the honest signal (raw screen count nags a genuine
  // long read). We accumulate time only between scroll events close in time; gaps
  // reset nothing but stop accruing. Generous default; once per page load (the
  // card's own guards suppress on reader-able pages and protected paths).
  const OVERSCROLL_MS = 70000; // ~70s of active scrolling past the fold
  let lastScrollTs = 0;
  let activeScrollMs = 0;
  let overscrollFired = false;
  function onScroll() {
    if (overscrollFired) return;
    const now = Date.now();
    if (lastScrollTs && now - lastScrollTs < 1500) activeScrollMs += now - lastScrollTs;
    lastScrollTs = now;
    if (activeScrollMs < OVERSCROLL_MS) return;
    // Reshape can be flipped on mid-session; only consume the trigger once we
    // actually fire, so enabling reshape on an open tab still lets it nudge.
    reshapeOn((on) => { if (on) { overscrollFired = true; raise('overscroll'); } });
  }
  window.addEventListener('scroll', onScroll, { passive: true });

  // ── Background-driven triggers (redirect / idle-return) ──────────────────────
  // features/reshape/background.js already checked reshape + distraction (and
  // re-validated the tab) before signaling — just raise the card.
  EXT.runtime.onMessage.addListener((msg) => {
    if (!msg || msg.type !== 'amdion-nudge') return;
    raise(msg.reason);
  });
})();
