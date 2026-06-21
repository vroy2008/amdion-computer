// Amdion reshape gate — the substrate every "calm the trap" item keys off.
//
// Reshaping (declutter + feed-fade + the behavioral nudges) is a per-site switch
// independent of the friction level: a site can be calmed even in Off mode
// (docs/REORIENTATION.md §9). This script runs at document_start, reads the
// pushed config, and toggles classes on <html>:
//
//   html.amdion-reshape   → this host is reshaped (declutter.css keys off this)
//   html.amdion-feedfade  → feed-fade is enabled (content/feedfade.js)
//   html.amdion-yt-home   → hide the YouTube home grid (declutter.css)
//
// CSS reads the classes; the JS scripts (nudge over-scroll, feedfade, ytdrift)
// read `globalThis.__amdionReshape`, kept current here, with an
// `__amdionReshapeReady` promise for scripts that want to wait for the first
// resolve. Default-on for the distraction set means the always-on declutter that
// shipped keeps working with no regression; a brief unstyled flash before the
// first storage read is the accepted cost of a permissionless, file-free gate.

(() => {
  const EXT = typeof chrome !== 'undefined' && chrome.runtime && chrome.runtime.id ? chrome : null;
  if (!EXT) return;

  const BUILTIN_DISTRACTIONS = [
    'youtube.com', 'twitter.com', 'x.com', 'facebook.com', 'instagram.com',
    'reddit.com', 'tiktok.com', 'netflix.com', 'twitch.tv',
  ];
  const HOST = location.hostname.replace(/^www\./, '');
  // twitter.com and x.com are one site; canonicalize so a single toggle covers both.
  const CANON = HOST === 'twitter.com' || HOST.endsWith('.twitter.com') ? 'x.com' : HOST;
  // Registrable-domain label (instagram.com → "instagram", and m.instagram.com →
  // "instagram" too) so declutter.css can scope a site-specific rule and never dim
  // a same-named anchor on another site. Use the second-level label, not the
  // left-most, so subdomains don't mis-slug (these sites are all single-TLD .com).
  const SITE = (CANON.split('.').slice(-2)[0]) || CANON;

  // Shared, mutated-in-place so readers that hold the reference see live updates.
  const state = { on: false, feedFade: false, ytHome: false, host: HOST };
  globalThis.__amdionReshape = state;

  const onList = (host, list) =>
    (list || []).some((d) => host === d || host.endsWith('.' + d) || CANON === d);

  function isReshaped(reshape, distractions) {
    if (!reshape || reshape.enabled === false) return false;
    if (onList(HOST, reshape.disabledSites)) return false; // explicit opt-out
    return onList(HOST, distractions);
  }

  function apply(reshape, distractions) {
    const r = reshape || { enabled: true, disabledSites: [], feedFade: false, hideYoutubeHome: false };
    state.on = isReshaped(r, distractions || BUILTIN_DISTRACTIONS);
    state.feedFade = state.on && !!r.feedFade;
    state.ytHome = state.on && !!r.hideYoutubeHome;
    const root = document.documentElement;
    if (!root) return;
    root.classList.toggle('amdion-reshape', state.on);
    root.classList.toggle('amdion-feedfade', state.feedFade);
    root.classList.toggle('amdion-yt-home', state.ytHome);
    if (SITE) root.classList.add('amdion-site-' + SITE); // stable; never removed

  }

  let resolveReady;
  globalThis.__amdionReshapeReady = new Promise((res) => { resolveReady = res; });

  EXT.storage.local.get(['reshape', 'distractions'], (r) => {
    apply(r.reshape, r.distractions);
    resolveReady(state);
  });
  EXT.storage.onChanged.addListener((changes, area) => {
    if (area !== 'local' || (!changes.reshape && !changes.distractions)) return;
    EXT.storage.local.get(['reshape', 'distractions'], (r) => apply(r.reshape, r.distractions));
  });
})();
