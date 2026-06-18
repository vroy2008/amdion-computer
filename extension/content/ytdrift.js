// Amdion YouTube drift counter — catch the video→suggested→suggested rabbit
// hole without touching legitimate viewing (docs/REORIENTATION.md §6).
//
// YouTube is a single-page app: watch→watch hops don't reload the page and don't
// fire webNavigation.onCommitted, so this can't live in the background worker. We
// listen to YouTube's own `yt-navigate-finish` event and count consecutive
// watch→watch hops. A search, a Subscriptions visit, or going home resets the
// counter — those are intentional, so only an uninterrupted chain accrues. After
// a generous N hops we raise one calm "Still on track?" card (reusing the shared
// nudge), then reset. In-memory integer only; never logged.

(() => {
  const EXT = typeof chrome !== 'undefined' && chrome.runtime && chrome.runtime.id ? chrome : null;
  if (!EXT) return;

  const HOST = location.hostname.replace(/^www\./, '');
  const N = 7; // generous — a few suggested clicks are fine; a chain is the trap
  // A YouTube video page's pathname is ALWAYS exactly "/watch" — the video id
  // lives in the query string (?v=…). So a hop must be detected by the v param,
  // not the path; comparing pathnames would never see a change.
  const videoId = () => new URLSearchParams(location.search).get('v');
  let hops = 0;
  let lastWasWatch = location.pathname.startsWith('/watch');
  let lastId = lastWasWatch ? videoId() : null;

  // Is YouTube reshaped right now? Prefer reshape.js's shared state; fall back to
  // storage. Drift is friction-independent, gated only on the reshape switch.
  function reshapeOn(cb) {
    const live = globalThis.__amdionReshape;
    if (live && typeof live.on === 'boolean' && live.host === HOST) return cb(live.on);
    EXT.storage.local.get(['reshape', 'distractions'], (r) => {
      const reshape = r.reshape || { enabled: true, disabledSites: [] };
      if (reshape.enabled === false) return cb(false);
      const disabled = (reshape.disabledSites || []).some((d) => HOST === d || HOST.endsWith('.' + d));
      cb(!disabled); // youtube.com is always in the distraction set
    });
  }

  function onNav() {
    const isWatch = location.pathname.startsWith('/watch');
    const id = isWatch ? videoId() : null;
    if (isWatch && lastWasWatch && id && id !== lastId) {
      hops += 1; // a fresh video reached from another video (v param changed)
      if (hops >= N) {
        reshapeOn((on) => {
          if (on && globalThis.__amdionNudge) globalThis.__amdionNudge.show({ reason: 'drift' });
          hops = 0; // reset so we don't fire on every subsequent hop
        });
      }
    } else if (!isWatch) {
      hops = 0; // left the rabbit hole (search / home / subscriptions) → reset
    }
    lastId = id;
    lastWasWatch = isWatch;
  }

  window.addEventListener('yt-navigate-finish', onNav);
})();
