// Amdion — the three modes (Off / Nudge / Block) and the distraction set.
//
// The effective mode lives in chrome.storage.local `friction.level`
// (off=Off / soft=Nudge / lockin=Block) and is driven by three sources, in
// precedence order (V1.md §3.2):
//   1. seed `soft` on install — the disconnected default (Defend gently-on).
//   2. the action popup — a MANUAL, session-scoped override (modeSource:manual).
//   3. the desktop app's `intent_mode` push — the intent default writer.
// Arbitration (applyIntentMode) keeps a manual override from being clobbered by
// a mere reconnect re-sync, while letting a new session or a changed/just-picked
// intent re-assert the intent default.
//
// Block (lockin) is declarativeNetRequest dynamic rules that redirect main-frame
// requests to known distractions onto blocked.html. Nudge (soft) and Block both
// act on BUILTIN_DISTRACTIONS ∪ the user's block list. Tracking is independent of
// the mode (always on). Everything is recomputed from storage (not memory), so a
// worker restart / reconnect / any mode change self-heals — a wrap can never
// strand the user.

import { onBridge } from './bridge.js';

/// The three user-facing modes ↔ the friction levels they map onto.
export const MODES = ['off', 'soft', 'lockin'];
const isMode = (v) => MODES.includes(v);

// Built-in distraction set. The user's block list (pushed by the app) is unioned
// in by applyBlockList. Editable additions live in Amdion's settings.
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

// The app's block-list push. Carries ONLY the distraction set now (the mode/level
// is owned by the popup + intent_mode contract, never by this message), so editing
// the block list never disturbs the current mode. Content scripts read
// `distractions` (and react to storage changes) for the Nudge card.
async function applyBlockList(payload) {
  const blockList = Array.isArray(payload.blockList) ? payload.blockList : [];
  const distractions = [...new Set([...BUILTIN_DISTRACTIONS, ...blockList])];
  const cur = (await chrome.storage.local.get('friction')).friction || {};
  await chrome.storage.local.set({
    friction: { level: cur.level || 'soft', blockList },
    distractions,
  });
  await refreshBlocking();
}

// Set the mode from the action popup — a MANUAL, session-scoped override. We mark
// `modeSource:'manual'` but deliberately don't touch lastSeen{Token,Intent}: those
// track the app's last push, and the arbitration below compares against them to
// decide when this override is allowed to be re-asserted away.
export async function applyManualMode(level) {
  if (!isMode(level)) return;
  const cur = (await chrome.storage.local.get('friction')).friction || {};
  await chrome.storage.local.set({
    friction: { level, blockList: cur.blockList || [] },
    modeSource: 'manual',
  });
  await refreshBlocking();
}

// The intent → mode contract (V1.md §3.2). The app pushes the mode mapped from the
// session's intent (or `off` when no intent is set), tagged with the session
// `token`, the `intent` label, and an `assert` flag:
//   • assert:true  — an explicit user action (an intent pick/clear). ALWAYS
//                    re-asserts, clearing any manual override.
//   • assert:false — a passive re-sync (sent at connect/reconnect). Won't clobber
//                    a manual override UNLESS the session or the intent changed —
//                    so a reconnect re-push of the same intent is a no-op, but a
//                    new session / changed intent re-asserts the intent default.
export async function applyIntentMode(payload) {
  const level = isMode(payload.level) ? payload.level : 'off';
  const token = payload.token ?? null;
  const intent = payload.intent ?? null;
  const assert = payload.assert === true;
  const st = await chrome.storage.local.get([
    'friction', 'modeSource', 'lastSeenToken', 'lastSeenIntent',
  ]);

  let apply;
  if (assert) {
    apply = true;
  } else {
    // A token we've genuinely seen before that now differs ⇒ a new session.
    // (The first push after install has lastSeenToken null — NOT a new session,
    // so a manual choice made while disconnected survives the first connect.)
    const newSession = st.lastSeenToken != null && token !== st.lastSeenToken;
    const intentChanged = intent !== (st.lastSeenIntent ?? null);
    apply = st.modeSource === 'manual' ? newSession || intentChanged : true;
  }

  const next = { lastSeenToken: token, lastSeenIntent: intent };
  if (apply) {
    const cur = st.friction || {};
    next.friction = { level, blockList: cur.blockList || [] };
    next.modeSource = 'intent';
  }
  await chrome.storage.local.set(next);
  if (apply) await refreshBlocking();
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

onBridge('block_list', applyBlockList);
onBridge('intent_mode', applyIntentMode);
