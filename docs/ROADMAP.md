# Amdion — Roadmap & Technical Plan

> **Source of truth.** This doc supersedes the technical sections of [focused_computer.md](focused_computer.md) (which remains the product/philosophy concept). For the concrete build steps, see [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md).
>
> _Last updated: 2026-06-13._

---

## Vision

**Amdion is one calm focus layer over your Mac.** It tunes your machine and your real browser for attention, senses how you're actually working, responds with exactly the amount of friction you chose, and quietly keeps an honest record — built so an AI agent can plug into all of it later.

While the rest of the industry builds technology that captures attention, Amdion defaults to calm and makes distraction an active choice rather than a passive drift.

**Target user:** Knowledge workers who spend 6+ hours/day in a browser, feel busy all day, and can't account for where the time went.

**What Amdion is NOT:** It is not an OS shell replacement (macOS won't allow that without disabling SIP), and it is **not** an embedded browser you live inside. It's a focus *layer* — a calm front door you summon, plus a companion that decorates and observes the real tools you already use.

---

## The Stack

Amdion is best understood as six layers. The first three ship in v1; the agent is the horizon.

| Layer | What it does | v1 |
|---|---|---|
| **Front door** | Switchable surface: ephemeral launchpad by default (summon → set off → work → return), with a pinnable minimal HUD option. Also the onboarding + settings home. | Step 1 |
| **Setup & tuning** | Guided first-run that declutters the machine: a Mac productivity-settings pass + a Chrome decluttering walkthrough. Automate where possible, overlay-and-point where not. | Steps 1–2 |
| **Sensing layer** | The keystone. Senses computer use and classifies it into **sessions / blocks / breaks** from activity vs. inactivity. Optional greeting-style intent capture. | Foundation |
| **Response** | Configurable friction: **Off** (passive only), **Soft** (default — nudge on common traps), **Lock-In** (block social + a user list). | Step 2 |
| **Observer** | Mostly passive: daily logs + honest graphs, fed by OS activity and the Chrome extension. | Step 3 |
| **Agent (reserved)** | Not built yet, but the data + action layer is shaped so an LLM can later read state and act for the user. | Deferred |

---

## Architecture: control the real Chrome

The earlier plan embedded web apps inside the Amdion window via Tauri's `add_child()` webview. **That direction is abandoned.** Tauri's webview on macOS is WKWebView (WebKit), which cannot import a Chrome profile — passwords, extensions, cookies/sessions, and config all stay locked in Chromium's encrypted store. Re-logging into everything and losing extensions is a non-starter for adoption.

Instead, Amdion **controls and observes the user's real Chrome:**

```
┌─────────────────────────┐         ┌──────────────────────────┐
│  Amdion app (Tauri/Rust) │  WS    │  Chrome extension (MV3)   │
│  ─ front door / launcher │◄──────►│  ─ declutter content CSS  │
│  ─ sensing engine        │ local  │  ─ tab activity reporting │
│  ─ observer + SQLite     │ :port  │  ─ open/focus tabs        │
│  ─ settings / onboarding │        │  ─ nudge overlays         │
└─────────────────────────┘         └──────────────────────────┘
        │                                       
        ├─ macOS idle/active signal (system idle time)
        ├─ frontmost-app polling (NSWorkspace)
        └─ `defaults write` for Mac tuning (+ guided walkthrough for gated settings)
```

**Bridge decision:** the extension talks to the always-running Amdion app over a **localhost WebSocket** the app hosts, *not* Chrome Native Messaging. Native messaging launches the host as a per-connection subprocess, which fights with an app that's already running. A localhost WS is simpler and more robust here. AppleScript is a no-install fallback for basic "open URL / list tabs / focus tab."

**Why this wins:**
- Keeps the user's Chrome profile, passwords, and extensions intact.
- Distraction-stripping is what extensions are built for (per-domain content scripts + CSS).
- Browser activity monitoring needs **no macOS permission** (the extension sees `chrome.tabs` / `webNavigation` / `idle` directly).
- Clean cross-browser path later: Edge (Chromium), Firefox (WebExtensions), Safari (Safari Web Extension wrapper).

---

## The sensing ontology

Purely time/activity based. Three terms, sensible defaults, configurable in an advanced setting:

- **Break** — an inactivity (idle, no input) gap ≥ **5 min** (default). Shorter pauses are ignored.
- **Block** — a continuous run of activity, ended by a break.
- **Session** — a sequence of blocks; ends after inactivity ≥ **30 min** (default), with a new session starting on next activity. A day can have multiple sessions.

"Morning" is inferred from the user's timezone + local time. The sensing layer is the substrate the Observer reports on and the Coach reacts to.

---

## Intent & friction

**Intent capture is optional and arrives as a greeting**, not a form. At the start of each new session (i.e. after any prolonged break), Amdion may offer "Want to jot down what you're focusing on?" — dismissable, configurable cadence, and fully disable-able in settings. If the user opts in, the stated intent gives drift detection something to measure against and is the one thing the HUD surfaces.

**Friction is configurable** (the product's personality dial):
- **Off** — passive monitoring/logging only, no nudge.
- **Soft (default)** — a gentle, dismissable nudge only when the user switches to common distractions (social media, YouTube, etc.).
- **Lock-In** — actively block social media plus a user-configured list (focus-mode-extension style).

---

## Setup & tuning layer (the novel wedge)

A guided first-run that makes the user's computer calmer in ~60 seconds. Split by feasibility:

**Mac settings — scriptable** (`defaults write` + restart the relevant process):
- Auto-hide the Dock; strip "recent apps"; shrink tile size
- Turn off app-icon badge counts where possible
- Reduce Motion / Reduce Transparency
- Hide desktop icons
- Calm hot-corner config

**Mac settings — walkthrough-only** (Apple-gated; overlay-and-point):
- Per-app notification toggles
- Setting up a macOS **Focus** mode
- Menu-bar decluttering
- Disabling Spotlight web / Siri suggestions
- Grayscale color filter (opt-in; powerful but opinionated)

**Chrome — via the extension/walkthrough:**
- Disable the new-tab news feed and recent-tab suggestions
- Install/enable uBlock Origin Lite
- "Small changes, huge attention impact."

---

## Build order

### Step 1 — Front door + Mac tuning onboarding
The launcher/desktop app shell, first-run onboarding, settings/options, and the Mac system-settings tuning pass.

### Step 2 — Clean-Chrome extension (simple version) + Response
MV3 extension + localhost WS bridge. Decluttering walkthrough first, then passive tab-activity reporting. Wire the configurable friction levels (Off / Soft / Lock-In).

### Step 3 — Sensing engine + Observer
macOS idle/active detection, the session/block/break classifier, SQLite event/session store, and the passive daily logs + graphs UI (ingesting Chrome data from Step 2).

### Deferred — AI Agent
Not implemented in v1, but every layer above exposes a clean, typed data + action surface so an LLM can later read state and act on the user's behalf with lean context.

---

## What we keep / cut / removed

| Decision | Detail |
|---|---|
| **CUT** embedded multi-webview | `open_app` → `add_child` no longer the model. `open_app` should drive the real Chrome (extension/AppleScript). |
| **CUT** Electron stack | `main.js`, `preload.js`, `gemini.js`, root `index.html`, `observer-mvp/` to be removed once Tauri reaches parity. |
| **CUT** screenshot-based journaling | Replaced by deterministic event logging (extension + OS idle/active). |
| **CUT** 10-step vision agent | The autonomous click/type/scroll agent is gone; agent becomes a deferred NL+actions layer. |
| **KEEP** four pillars + philosophy | From [focused_computer.md](focused_computer.md) — still valid. |
| **KEEP** Gemini chat, voice transcription, global hotkey, favorites, config | Foundation carries over to the new shape. |
| **ADD** Setup & tuning layer | New first-run pillar. |
| **ADD** sensing ontology + SQLite | New foundation. |
| **ADD** Chrome extension + WS bridge | New integration surface. |

---

## Target file structure

```
src-tauri/
├── src/
│   ├── main.rs
│   ├── lib.rs              # app setup, command registration only
│   ├── state.rs            # AppState
│   ├── config.rs           # config + favorites
│   ├── commands/
│   │   ├── browser.rs      # open/focus tab via Chrome (WS/AppleScript)
│   │   ├── chat.rs         # Gemini chat
│   │   ├── tuning.rs       # Mac defaults-write tuning + walkthrough state
│   │   ├── sensing.rs      # session/block/break queries
│   │   └── observer.rs     # daily logs, summaries, graph data
│   ├── bridge_ws.rs        # localhost WebSocket server for the extension
│   ├── sensing.rs          # idle/active engine + classifier (background thread)
│   ├── db.rs               # rusqlite: events, sessions, blocks
│   └── gemini.rs           # Gemini client
├── Cargo.toml
└── tauri.conf.json
frontend/
├── index.html              # front door + settings + observer UI
└── bridge.js               # Tauri ↔ electronAPI adapter (kept)
extension/                  # NEW — Chrome MV3 extension
├── manifest.json
├── background.js           # WS client, tab events, open/focus
├── content/                # declutter CSS + nudge overlays per domain
└── walkthrough.js          # guided Chrome decluttering
```

---

## Current status

See [README.md](../README.md#status) for the short version. In brief: the Electron→Tauri migration landed, the Tauri shell runs (window, favorites, Gemini chat, voice, global hotkey), but the codebase still reflects the *old* embedded-webview + screenshot-journal model. The work ahead is the re-point described above. Nothing in Steps 1–3 is built yet against the new architecture.
