# Amdion Computer

**One calm focus layer over your Mac.** Amdion tunes your machine and your real browser for attention, senses how you're actually working, responds with exactly the amount of friction you chose, and quietly keeps an honest record — built so an AI agent can plug into it later.

It's not an OS replacement and not a browser you live inside. It's a focus *layer*: a calm front door you summon, plus a companion extension that declutters and observes the real Chrome you already use.

![Amdion Logo](amdion_logo_new.png)

## The four pillars

- **The Interface** — a minimalist, text-only front door (no icons). Switchable between an ephemeral launchpad and a pinned minimal HUD.
- **The Assistant** — chat (Gemini) that helps you act without diving into the distraction zone.
- **The Coach** — rare, gentle, configurable nudges. Off / Soft / Lock-In.
- **The Observer** — passive session logging with honest daily logs and graphs.

Plus a **Setup & tuning** first-run that declutters your Mac and Chrome in ~60 seconds.

See [docs/ROADMAP.md](docs/ROADMAP.md) for the vision and architecture, and [docs/IMPLEMENTATION_PLAN.md](docs/IMPLEMENTATION_PLAN.md) for the build plan.

## Status

> Live progress log: [STATUS.md](STATUS.md). The summary below is the high-level picture.

The app migrated from Electron to **Tauri v2** (Rust backend, native webview frontend; ~12.7 MB binary). The Tauri shell runs today: window/fullscreen, text favorites, Gemini chat, voice transcription, global hotkey (`Cmd+Shift+Space`), config.

**Re-pointing in progress.** The codebase still reflects the old "embedded browser + screenshot journal" model, which has been abandoned (see ROADMAP → *What we keep / cut / removed*). The current work is the v1 in the implementation plan:

1. **Front door + Mac tuning onboarding** — not started
2. **Clean-Chrome extension + configurable friction** — not started
3. **Sensing engine (sessions/blocks/breaks) + Observer** — not started

The AI agent is deferred but the architecture reserves a clean data/action surface for it.

## Architecture

- **Tauri v2** — Rust backend, native window management, system idle/active sensing, SQLite event store.
- **Chrome extension (MV3)** — declutters the user's real Chrome, reports tab activity, opens/focuses tabs. Talks to the app over a **localhost WebSocket** (AppleScript fallback).
- **Gemini** (`reqwest`) — chat and audio transcription.
- **Vanilla frontend** — HTML/CSS/JS front door, settings, and observer UI.

## Setup

```bash
# Frontend deps (legacy Electron tooling still present during migration)
npm install

# Gemini key — via env or config.json
echo "GEMINI_API_KEY=your_api_key_here" > .env

# Run the Tauri app (requires Rust toolchain)
cd src-tauri && cargo tauri dev
```

## Project structure

```
src-tauri/            # Tauri v2 Rust backend (app, sensing, db, bridge, gemini)
frontend/             # Front door + settings + observer UI, bridge.js adapter
extension/            # Chrome MV3 extension (planned — declutter, report, control)
docs/                 # ROADMAP, IMPLEMENTATION_PLAN, product concept
```

---

*Built by [AMDION](https://amdion.org) — Time is your most valuable asset. We help you protect it.*
