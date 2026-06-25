// Amdion feed-fade — dim the bottomless feed past the fold so skimming forever
// takes a deliberate choice, not zero friction.
//
// EXPERIMENT-TIER, DEFAULT OFF. Opacity-only (no grayscale); the first items you
// see stay sharp, items past the fold dim; an explicit CLICK restores one (never
// :hover — a trackpad un-fades the whole feed while skimming). After a large run
// of items, a calm "you're caught up for now" end-cap (the sentinel-free take on
// the infinite-feed stop — we don't fight the site's load-more sentinel, which
// drifts fastest on exactly these surfaces). Fail-safe: if the item selector
// matches nothing, this does nothing.
//
// Gated on the reshape switch AND the feed-fade sub-toggle (html.amdion-feedfade,
// published by content/reshape.js).

(() => {
  const EXT = typeof chrome !== 'undefined' && chrome.runtime && chrome.runtime.id ? chrome : null;
  if (!EXT) return;
  if (window.top !== window) return; // top frame only
  const NS = (window.__amdion = window.__amdion || {});

  const HOST = location.hostname.replace(/^www\./, '');
  const SITE = HOST === 'twitter.com' || HOST.endsWith('.twitter.com') ? 'x'
             : HOST.split('.')[0];
  // Per-site feed shape. `item`: a feed card. `needs`: a descendant that marks a
  // real post (skip the pills / who-to-follow cells). `container`: where to watch
  // for appended items and where to drop the end-cap.
  const SITES = {
    x: { item: '[data-testid="cellInnerDiv"]', needs: 'article', container: '[data-testid="primaryColumn"]' },
    linkedin: { item: '.feed-shared-update-v2', needs: null, container: 'main' },
  };
  const CFG = SITES[SITE];
  if (!CFG) return;

  const FOLD = 8;   // items kept sharp before fading begins
  const CAPAT = 40; // items before the "caught up" end-cap

  let running = false;
  let viewed = 0;
  let cappedShown = false;
  let io = null;
  let mo = null;
  let styleEl = null;
  let clickBound = false;

  function injectStyle() {
    if (styleEl) return;
    styleEl = document.createElement('style');
    styleEl.id = 'amdion-feedfade-style';
    styleEl.textContent = `
      .amdion-faded { opacity: .3 !important; transition: opacity .25s ease !important; cursor: pointer; }
      .amdion-feed-endcap { text-align:center; color:#8a857c; font:14px -apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;
        padding:26px 16px; opacity:.85; }
      .amdion-feed-endcap b { color:#2480ba; font-weight:600; letter-spacing:.04em; }`;
    (document.head || document.documentElement).appendChild(styleEl);
  }

  const isItem = (n) => n && n.nodeType === 1 && n.matches && n.matches(CFG.item) && (!CFG.needs || n.querySelector(CFG.needs));

  function observeItem(n) {
    if (n.__amdionFade) return;
    n.__amdionFade = true;
    if (io) io.observe(n);
  }

  function scan(root) {
    const r = root && root.querySelectorAll ? root : document;
    r.querySelectorAll(CFG.item).forEach((n) => { if (isItem(n)) observeItem(n); });
  }

  function endcap(container) {
    if (cappedShown) return;
    cappedShown = true;
    const cap = document.createElement('div');
    cap.className = 'amdion-feed-endcap';
    cap.innerHTML = `<b>AMDION</b><br>You're caught up for now.`;
    (container || document.body).appendChild(cap);
  }

  function onIntersect(entries) {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      const n = e.target;
      if (!n.__amdionCounted) { n.__amdionCounted = true; viewed += 1; }
      if (viewed > FOLD && !n.__amdionClicked) n.classList.add('amdion-faded');
      if (viewed >= CAPAT) endcap(document.querySelector(CFG.container));
    }
  }

  // An explicit click on a faded card restores it (and proceeds with whatever the
  // click does). pointerdown so it fires before navigation.
  function onPointerDown(e) {
    const card = e.target.closest && e.target.closest('.amdion-faded');
    if (!card) return;
    card.__amdionClicked = true;
    card.classList.remove('amdion-faded');
  }

  function start() {
    if (running) return;
    running = true;
    injectStyle();
    io = new IntersectionObserver(onIntersect, { threshold: 0.4 });
    const container = document.querySelector(CFG.container) || document.body;
    mo = new MutationObserver((muts) => {
      for (const m of muts) m.addedNodes && m.addedNodes.forEach((n) => {
        if (isItem(n)) observeItem(n);
        else if (n.nodeType === 1 && n.querySelector) scan(n);
      });
    });
    mo.observe(container, { childList: true, subtree: true });
    scan(document);
    if (!clickBound) { document.addEventListener('pointerdown', onPointerDown, true); clickBound = true; }
  }

  function stop() {
    if (!running) return;
    running = false;
    if (io) { io.disconnect(); io = null; }
    if (mo) { mo.disconnect(); mo = null; }
    document.querySelectorAll('.amdion-faded').forEach((n) => n.classList.remove('amdion-faded'));
    // Clear the per-node markers too, so a later re-enable re-observes every card
    // (observeItem early-returns on __amdionFade; a fresh IO must see them again).
    document.querySelectorAll(CFG.item).forEach((n) => {
      delete n.__amdionFade; delete n.__amdionCounted; delete n.__amdionClicked;
    });
    const cap = document.querySelector('.amdion-feed-endcap');
    if (cap) cap.remove();
    viewed = 0; cappedShown = false;
  }

  // Enabled = reshape on for this site AND the feed-fade sub-toggle. reshape.js
  // publishes both as classes on <html>; mirror them here and react to changes.
  function enabled() {
    const root = document.documentElement;
    return root.classList.contains('amdion-reshape') && root.classList.contains('amdion-feedfade');
  }
  function sync() { if (enabled()) start(); else stop(); }

  // reshape.js resolves async; wait for it, then track class changes on <html>.
  (NS.reshapeReady || Promise.resolve()).then(sync);
  new MutationObserver(sync).observe(document.documentElement, { attributes: true, attributeFilter: ['class'] });
})();
