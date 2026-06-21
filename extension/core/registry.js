// Amdion — feature registry + enable-gate.
//
// The stable core (bridge, block, track) lives in core/. Each bonus feature
// lives in features/<name>/ and self-registers here at import time. This buys two
// things: (1) core never imports a feature's internals — it only dispatches named
// lifecycle hooks, so a feature can be edited in isolation; (2) a single enable
// map (mirrored from chrome.storage.local 'features') gates every feature.
//
// V1 = the simplified spine: bonus features are DORMANT by default (absent flag ⇒
// off). A feature runs only when its flag is explicitly true. The gate is
// load-bearing on two paths: dispatch() skips a disabled feature's hooks, and
// core/background.js only registers an ENABLED feature's content scripts (see
// enabledContentScripts) — so a dormant feature contributes no code at all, in the
// worker or on the page.

const features = [];
let enabledMap = {};

// def: { name, defaults?, hooks? } — hooks is { hookName: fn }.
export function registerFeature(def) {
  features.push(def);
}

export function setEnabledMap(map) {
  enabledMap = map || {};
}

export function isEnabled(name) {
  return enabledMap[name] === true; // default OFF — dormant until explicitly unlocked
}

// Fan a named lifecycle hook out to every enabled feature that implements it.
// Faults are contained: one feature throwing never breaks core or its siblings.
export function dispatch(hook, ...args) {
  for (const f of features) {
    if (!isEnabled(f.name)) continue;
    const fn = f.hooks && f.hooks[hook];
    if (typeof fn !== 'function') continue;
    try { fn(...args); } catch (e) { console.warn('[amdion] feature', f.name, hook, 'failed:', e); }
  }
}

// Merge every feature's seed defaults (used once on install).
export function featureDefaults() {
  return features.reduce((acc, f) => Object.assign(acc, f.defaults || {}), {});
}

// The content scripts each ENABLED feature wants injected, flattened. Each spec is
// a chrome.scripting RegisteredContentScript ({ id, matches, js?/css?, runAt }).
// core/background.js reconciles these with the live registration set, so a dormant
// feature's content scripts are never on the page.
export function enabledContentScripts() {
  return features.filter((f) => isEnabled(f.name)).flatMap((f) => f.contentScripts || []);
}
