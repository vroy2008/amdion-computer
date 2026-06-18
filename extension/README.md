# Amdion Companion (Chrome MV3 extension)

The browser half of Amdion. It connects to the always-running Amdion app over a
localhost WebSocket, reports tab/idle activity, and applies the friction level
you choose in Amdion (Soft nudges / Lock-In blocking). All data stays on your
device.

## Load it (unpacked, dev)

1. Open `chrome://extensions` in Chrome.
2. Turn on **Developer mode** (top-right).
3. Click **Load unpacked** and choose this `extension/` folder.
4. Make sure the Amdion app is running — the walkthrough tab that opens will show
   **Connected to Amdion** once the bridge is up.

The extension's id is pinned by the `key` in `manifest.json`, so Amdion's bridge
can allowlist exactly this extension's origin
(`chrome-extension://kobehecgjgjgjlljidhjjlgadpdmnfbp`).

## Files

| File | Role |
|---|---|
| `manifest.json` | MV3 manifest — pinned `key`, minimal permissions, content scripts. |
| `background.js` | Service worker: WS client + `chrome.alarms` keepalive, tab/idle/navigation events, `open_tab`/`focus_tab`/`close_tab`, and Lock-In blocking via `declarativeNetRequest`. |
| `content/nudge.js` | Soft-mode dismissable nudge overlay (shadow DOM) on distraction domains. |
| `content/declutter.css` | Example per-domain "decorate" tweaks (YouTube Shorts, X trends). |
| `blocked.html` | Lock-In landing page. |
| `walkthrough.html` / `walkthrough.js` | First-run guide (opens on install) with a live connection indicator. |

## Protocol

JSON `{ type, payload }` over `ws://127.0.0.1:<port>` — the extension scans the
dev range (17883–17893) first, then the release range (17872–17882), so a running
`tauri dev` build is preferred over the installed app:

- **→ app:** `hello`, `tab_opened`, `tab_activated`, `tab_closed`, `tab_navigated`, `idle_state`, `ping`
- **← app:** `friction { level, blockList }`, `open_tab { url }`, `focus_tab { tabId }`, `close_tab { tabId }`
