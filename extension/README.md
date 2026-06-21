# Amdion Companion (Chrome MV3 extension)

The browser half of Amdion. It connects to the running Amdion app over a
loopback WebSocket, reports tab/idle activity, and applies the mode you
choose — **Off / Nudge / Block** — on a set of distraction sites. All data
stays on your device.

> Scope of truth for what's in V1 is the root [README](../README.md) and
> [docs/V1.md](../docs/V1.md).

## Load it (unpacked, dev)

1. Open `chrome://extensions` in Chrome.
2. Turn on **Developer mode** (top-right).
3. Click **Load unpacked** and choose this `extension/` folder.
4. Make sure the Amdion app is running — the toolbar popup shows **Connected**
   once the bridge is up.

The extension's id is pinned by the `key` in `manifest.json`, so Amdion's bridge
can allowlist exactly this extension's origin
(`chrome-extension://kobehecgjgjgjlljidhjjlgadpdmnfbp`).

## Files

The extension is split into `core/` (the V1 spine) and `features/` (the gated
"bonus shelf" — see the root README's *Beyond V1*).

| Path | Role |
|---|---|
| `manifest.json` | MV3 manifest — pinned `key`, minimal permissions, content scripts. |
| `popup.html` / `popup.js` | Toolbar popup: the manual **Off / Nudge / Block** toggle + connection status. |
| `core/background.js` | Service worker: bridge client + `chrome.alarms` keepalive, tab/idle/navigation events, and `open_tab`/`focus_tab`/`close_tab`. |
| `core/bridge.js` | WebSocket client + the `{ type, payload }` message envelope to/from the app. |
| `core/block.js` | Mode arbitration (Off/Nudge/Block) + **Block** redirects via `declarativeNetRequest`. |
| `core/nudge.js` | **Nudge** dismissable in-page card (shadow DOM) on distraction domains. |
| `core/registry.js` | Feature enable-map that gates the `features/` bonus shelf. |
| `features/` | Bonus shelf: `read-mode/`, `notes/`, `present/`, `reshape/`. Not part of the V1 spine. |
| `blocked.html` | **Block**-mode landing page. |
| `walkthrough.html` / `walkthrough.js` | First-run guide with a live connection indicator. |

## Protocol

JSON `{ type, payload }` over `ws://127.0.0.1:<port>` — the extension scans the
dev range (17883–17893) first, then the release range (17872–17882), so a running
`tauri dev` build is preferred over the installed app:

- **→ app:** `hello`, `tab_opened`, `tab_activated`, `tab_closed`, `tab_navigated`, `idle_state`, `ping` (plus `read_started` / `read_ended` / `note_captured` from the bonus features).
- **← app:** `block_list { blockList }`, `intent_mode { level, token, intent, assert }`, `intent { intent }`, `open_tab { url }`, `focus_tab { tabId }`, `close_tab { tabId }` (plus `read_mode` / `read_prefs` / `capture_tab` / `present_mode` for the bonus features).
