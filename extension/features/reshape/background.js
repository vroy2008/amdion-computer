// Amdion feature: reshape — the per-site "calm the trap" layer plus its two
// background-driven nudge signals. The app pushes the reshape config (mirrored to
// storage where the content scripts read it); the redirect-chase and idle-return
// signals live here because the navigation qualifiers / idle transitions are only
// visible in the worker. Both hand off to the in-page nudge via a runtime message.

import { onBridge } from '../../core/bridge.js';
import { hostOf, onDistraction, BUILTIN_DISTRACTIONS } from '../../core/block.js';
import { registerFeature, isEnabled } from '../../core/registry.js';

// Default-on for known trap sites via an opt-out list; the aggressive feed-hiding
// items default off. Seeded on install so content scripts have it before connect.
const DEFAULT_RESHAPE = { enabled: true, disabledSites: [], feedFade: false, hideYoutubeHome: false };

function applyReshape(payload) {
  const reshape = {
    enabled: payload.enabled !== false,
    disabledSites: Array.isArray(payload.disabledSites) ? payload.disabledSites : [],
    feedFade: !!payload.feedFade,
    hideYoutubeHome: !!payload.hideYoutubeHome,
  };
  chrome.storage.local.set({ reshape });
}

// Is this host currently reshaped? Reshaping applies to the distraction set when
// the master switch is on and the site hasn't been opted out. twitter.com
// canonicalizes to x.com. Mirrors features/reshape/reshape.js so background-driven
// nudges agree with the content-side gate.
function isReshaped(host, reshape, distractions) {
  if (!host || !reshape || reshape.enabled === false) return false;
  const canon = host === 'twitter.com' || host.endsWith('.twitter.com') ? 'x.com' : host;
  const disabled = (reshape.disabledSites || []).some(
    (d) => host === d || host.endsWith('.' + d) || canon === d
  );
  if (disabled) return false;
  return onDistraction(host, distractions);
}

// Tell the in-page nudge to show a behavioral card. Best-effort: a missing
// receiver (no content script yet) just no-ops.
function sendNudge(tabId, reason) {
  if (!Number.isInteger(tabId)) return;
  chrome.tabs.sendMessage(tabId, { type: 'amdion-nudge', reason }, () => void chrome.runtime.lastError);
}

// Redirect / ad-chase → "hold for later". Fires only on a true client/server
// redirect landing on a reshape-on distraction — not on plain link clicks. The
// nudge shows once per page load.
async function maybeRedirectNudge(d) {
  const quals = d.transitionQualifiers || [];
  if (!quals.includes('client_redirect') && !quals.includes('server_redirect')) return;
  const host = hostOf(d.url);
  const { reshape, distractions } = await chrome.storage.local.get(['reshape', 'distractions']);
  if (!isReshaped(host, reshape || DEFAULT_RESHAPE, distractions || BUILTIN_DISTRACTIONS)) return;
  // The content script lands at document_idle; give it a beat to register its
  // message listener, then RE-VALIDATE the tab still shows a reshaped distraction
  // before signaling (it may have navigated away — the content side trusts us).
  setTimeout(async () => {
    if (!Number.isInteger(d.tabId)) return;
    let tab;
    try { tab = await chrome.tabs.get(d.tabId); } catch (_) { return; }
    if (!tab) return;
    const cur = await chrome.storage.local.get(['reshape', 'distractions']);
    if (isReshaped(hostOf(tab.url || ''), cur.reshape || DEFAULT_RESHAPE, cur.distractions || BUILTIN_DISTRACTIONS)) {
      sendNudge(d.tabId, 'redirect');
    }
  }, 700);
}

// Idle-return re-anchor: coming back from idle/lock onto an already-open
// distraction tab, re-offer the nudge once.
async function maybeIdleReturnNudge() {
  const { reshape, distractions } = await chrome.storage.local.get(['reshape', 'distractions']);
  const tabs = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  const tab = tabs && tabs[0];
  if (!tab || !Number.isInteger(tab.id)) return;
  const host = hostOf(tab.url || '');
  if (!isReshaped(host, reshape || DEFAULT_RESHAPE, distractions || BUILTIN_DISTRACTIONS)) return;
  sendNudge(tab.id, 'idle-return');
}

onBridge('reshape', (p) => { if (isEnabled('reshape')) applyReshape(p); });
registerFeature({
  name: 'reshape',
  defaults: { reshape: DEFAULT_RESHAPE },
  hooks: { onNavCommitted: maybeRedirectNudge, onActiveFromIdle: maybeIdleReturnNudge },
  contentScripts: [
    { id: 'reshape', matches: ['<all_urls>'], js: ['features/reshape/reshape.js'], runAt: 'document_start' },
    { id: 'reshape-nudge-triggers', matches: ['<all_urls>'], js: ['features/reshape/nudge-triggers.js'], runAt: 'document_idle' },
    { id: 'reshape-feedfade', matches: ['*://*.x.com/*', '*://*.twitter.com/*', '*://*.linkedin.com/*'], js: ['features/reshape/feedfade.js'], runAt: 'document_idle' },
    { id: 'reshape-ytdrift', matches: ['*://*.youtube.com/*'], js: ['features/reshape/ytdrift.js'], runAt: 'document_idle' },
    { id: 'reshape-declutter', matches: ['*://*.youtube.com/*', '*://*.x.com/*', '*://*.twitter.com/*', '*://*.linkedin.com/*', '*://*.instagram.com/*'], css: ['features/reshape/declutter.css'], runAt: 'document_start' },
  ],
});
