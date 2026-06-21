// Amdion — friction + Lock-In blocking (the "Block" mode) and the distraction set.
//
// Block is the lockin level: declarativeNetRequest dynamic rules that redirect
// main-frame requests to known distractions onto blocked.html. Nudge (soft) and
// Block both act on BUILTIN_DISTRACTIONS ∪ the user's block list. Everything is
// recomputed from storage (not memory), so a worker restart / reconnect / any
// friction push self-heals — a wrap can never strand the user.

import { onBridge } from './bridge.js';

// Built-in distraction set. The user's block list (pushed by the app) is unioned
// in by applyFriction. Editable additions live in Amdion's settings.
export const BUILTIN_DISTRACTIONS = [
  'youtube.com', 'twitter.com', 'x.com', 'facebook.com', 'instagram.com',
  'reddit.com', 'tiktok.com', 'netflix.com', 'twitch.tv',
];

// Reserved id range for our dynamic blocking rules, so we only ever remove ours.
const RULE_BASE = 9000;

export function hostOf(url) {
  try { return new URL(url).hostname.replace(/^www\./, ''); } catch (_) { return ''; }
}

export function onDistraction(host, distractions) {
  if (!host) return false;
  return (distractions || []).some((d) => host === d || host.endsWith('.' + d));
}

async function applyFriction(payload) {
  const level = payload.level || 'off';
  const blockList = Array.isArray(payload.blockList) ? payload.blockList : [];
  const distractions = [...new Set([...BUILTIN_DISTRACTIONS, ...blockList])];
  // Content scripts read these (and react to storage changes) for Soft nudges.
  await chrome.storage.local.set({ friction: { level, blockList }, distractions });
  await refreshBlocking();
}

// Recompute Lock-In blocking from the *effective* level: the user's base
// friction, escalated to lockin while a read/present wrap is open. Driving every
// rebuild through one function over storage means snapshot/restore is implicit
// (the base level IS the snapshot) and self-healing.
export async function refreshBlocking() {
  const { friction, distractions, readingLock } = await chrome.storage.local.get([
    'friction', 'distractions', 'readingLock',
  ]);
  const base = (friction && friction.level) || 'off';
  const set = Array.isArray(distractions) ? distractions : BUILTIN_DISTRACTIONS;
  await rebuildBlockingRules(readingLock ? 'lockin' : base, set);
}

// Raise/lower "the wrap" — the read/present temporary escalation to lockin.
// Persisted (survives a worker restart) and applied via the recompute above.
export async function setReadingLock(on) {
  await chrome.storage.local.set({ readingLock: !!on });
  await refreshBlocking();
}

async function rebuildBlockingRules(level, distractions) {
  const existing = await chrome.declarativeNetRequest.getDynamicRules();
  const ourIds = existing
    .filter((r) => r.id >= RULE_BASE && r.id < RULE_BASE + 1000)
    .map((r) => r.id);

  let addRules = [];
  if (level === 'lockin') {
    addRules = distractions.map((domain, i) => ({
      id: RULE_BASE + i,
      priority: 1,
      action: {
        type: 'redirect',
        redirect: { url: chrome.runtime.getURL('blocked.html') + '?d=' + encodeURIComponent(domain) },
      },
      // requestDomains matches the domain and its subdomains (www., m., …).
      condition: { requestDomains: [domain], resourceTypes: ['main_frame'] },
    }));
  }
  await chrome.declarativeNetRequest.updateDynamicRules({ removeRuleIds: ourIds, addRules });
}

onBridge('friction', applyFriction);
