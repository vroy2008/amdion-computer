// Amdion — feature registry + enable-gate.
//
// The stable core (bridge, block, track) lives in core/. Each bonus feature
// lives in features/<name>/ and self-registers here at import time. This buys two
// things: (1) core never imports a feature's internals — it only dispatches named
// lifecycle hooks, so a feature can be edited in isolation; (2) a single enable
// map (mirrored from chrome.storage.local 'features') can turn a feature dormant.
// In V1 every feature is enabled by default (absent flag ⇒ on), so today this is
// structure only — no behavior change.

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
  return enabledMap[name] !== false; // default ON
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
