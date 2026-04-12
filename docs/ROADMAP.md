# Amdion — Product Roadmap & Technical Plan

## Vision

Amdion is an attention guardian for your computer. While the rest of the industry builds technology that captures attention, Amdion defaults to calm and makes distraction an active choice rather than a passive drift.

**Core promise:** Tell Amdion what you're doing. It keeps the noise out, honestly shows you what happened, and gets smarter over time about your real patterns.

**Target user:** Knowledge workers who spend 6+ hours/day in a browser, feel busy all day, and can't account for where the time went.

---

## Architecture: Tauri v2

Built on Tauri v2 (Rust backend, native webview frontend). Chosen over Electron for:
- 10x smaller binary (~5MB vs ~200MB)
- 5x less RAM (~50MB vs ~300MB)
- Sub-second startup
- Rust backend for system-level features

### Key Tauri capabilities used
- **Embedded browser** via `window.add_child()` — multiple isolated webviews in one window
- **Local SQLite** via `rusqlite` for event log and episode storage
- **Background tasks** as Rust threads with timers
- **Global hotkeys** via `tauri-plugin-global-shortcut`
- **Screen capture** via `xcap` crate
- **System tray, auto-start, fullscreen** — all first-class in Tauri v2

### Frontend
- Shared HTML/CSS/JS frontend works on both Electron and Tauri via `bridge.js` adapter
- `bridge.js` maps `window.electronAPI.*` calls to Tauri `invoke()`/`listen()`

---

## Build Phases

### ✅ Phase 0: Existing Foundation (Done)
What already works in Tauri:
- Window creation, fullscreen, dark UI
- Text-based favorites launcher
- AI chat (text-only, via Gemini REST API)
- Config management (API key, model, favorites)
- Journal review UI shell
- Voice transcription
- Global shortcut (Cmd+Shift+Space)
- Tab state management

### 🔨 Phase 1: Embedded Browser (Current)
Replace separate-window app opening with embedded multi-webview.

**What:**
- `window.add_child()` to embed web apps inside the main window
- Tab show/hide/close via `set_visible()` and `close()`
- Resize handling when sidebars toggle
- `xcap` for screen capture (chat-with-screenshot)

**Why:** Core to the "single surface" experience. User should never leave Amdion.

### 📊 Phase 2: Deterministic Event Logging
Replace screenshot-based journal with structured event capture.

**What:**
- `rusqlite` database with `events` table
- Log: `tab_opened`, `tab_switched`, `tab_closed`, `home_returned`, `chat_query`
- Track timestamps and durations per tab
- Daily summary queries (time per app, switch count, total events)
- Frontend daily view showing raw activity stats

**Why:** "Real-time event logging — deterministic, passive capture of tabs, apps, time. No inference at write time." This is the honest data substrate.

### 🧠 Phase 3: Episode Parser
Background Gemini pass that turns raw events into meaningful episodes.

**What:**
- Rust background timer (every 3 hours)
- Reads raw events from SQLite
- Sends to Gemini: "Extract 3-10 meaningful work episodes"
- Stores episodes in SQLite `episodes` table
- Frontend daily summary showing episodes with duration bars

**Why:** "Periodic intent parser — every few hours, a background pass reads recent events and extracts meaningful episodes. Not per-click labeling." This powers the honest daily summary.

---

## Deferred — Needs Design Work

### 🎯 Intent Capture & Gate Control
- User states what they're working on ("writing quarterly report")
- Structured intent categories (Writing, Research, Email, Break, etc.)
- Allowed apps per intent
- Gentle friction when opening off-intent apps
- **Why deferred:** Subjective. Needs careful UX design to avoid being preachy or annoying.

### 🔔 Drift Detection
- Compare stated intent vs actual behavior
- Subtle nudge when divergence detected
- **Why deferred:** Depends on intent capture being designed first.

### 🎨 CSS Stripping / GenUI
- Strip distracting UI from embedded web apps (YouTube recs, Gmail promotions, LinkedIn feeds)
- Three-layer approach considered: seed rules → GenUI (DOM → Gemini → CSS) → cache
- Toggle switch — user opts in
- **Why deferred:** "Distracting" is subjective. Gemini prompt too generic. Needs testing to validate quality. The embedded browser works fine without it.

### 🤖 Agent Redesign
- Current: 10-step autonomous vision-agent loop (click/type/scroll)
- Vision: simpler NL commands ("open Gmail", "reply to Sarah: approved")
- **Why deferred:** Over-engineered for the vision. Simplify to chat commands first.

### 📈 Decision Trace Graph
- Cumulative internal graph of confirmed episodes across days
- Powers pattern recognition: "how do your writing sessions usually go?"
- **Why deferred:** Vision says "never exposed as a concept to users." Internal infrastructure — build after episodes are reliable.

---

## What We're Removing

| Removed | Replaced By |
|---|---|
| `journals/` JSON files (AI screenshot summaries) | SQLite `events` table (deterministic) |
| Separate-window app opening (`WebviewWindowBuilder`) | Embedded webviews (`add_child`) |
| User-facing knowledge graph UI | Internal only (per vision) |
| Tasks/notes on home screen | Simplified home → favorites only |
| Screenshot-based `summarizeActivity()` | Structured event logging |

---

## File Structure (Target)

```
src-tauri/
├── src/
│   ├── lib.rs          # App setup, Tauri command registration
│   ├── main.rs         # Entry point
│   ├── commands/       # Tauri commands by feature
│   │   ├── browser.rs  # open_app, switch_tab, close_tab, go_home
│   │   ├── config.rs   # get_config, save_config, favorites
│   │   ├── chat.rs     # send_chat_message
│   │   ├── events.rs   # get_daily_events, get_daily_summary
│   │   └── episodes.rs # get_episodes, trigger_parse
│   ├── db.rs           # EventDb — rusqlite wrapper
│   ├── gemini.rs       # Gemini API client
│   └── state.rs        # AppState struct
├── Cargo.toml
└── tauri.conf.json
frontend/
├── index.html          # Main UI
└── bridge.js           # Tauri ↔ electronAPI adapter
```
