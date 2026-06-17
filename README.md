# Amdion Computer

**One calm attention layer over your Mac.** Amdion tunes your machine and your real browser for attention, senses how you're actually working, responds with exactly the amount of friction you chose, and quietly keeps an honest record — built so an AI agent can plug into it later.

It's not an OS replacement and not a browser you live inside. It's an attention *layer*: a calm front door you summon from the menu bar, plus a companion extension that declutters and observes the real Chrome you already use.

![Amdion Logo](amdion_logo_new.png)

## The four pillars

- **The Interface** — a minimalist, text-only front door you summon with `⌘⇧Space`. Lives in the menu bar; no Dock icon, no window to manage.
- **The Assistant** — chat (Gemini) that helps you act without diving into the distraction zone.
- **The Coach** — rare, gentle, configurable friction. Off / Soft / Lock-In.
- **The Observer** — passive session logging with honest daily stats and graphs.

Plus **Read Mode** — a Kindle-like in-page reader that locks out distractions for the length of an article — and a **Setup & tuning** first-run that declutters your Mac and Chrome in ~60 seconds.

## Quick start

Amdion runs on **macOS + Chrome**. There's no one-click download yet (a signed build is in the works — see *Status*), so for now you build it from source. It takes about five minutes, most of which is the first Rust compile.

> **Installing with an AI coding agent?** Point it at this repo and tell it: *"Set up and run Amdion on my Mac — install any missing prerequisites, build the Tauri app, then help me load the Chrome extension."* Everything below is what it'll do.

**Prerequisites:** [Rust](https://rustup.rs) · [Node.js](https://nodejs.org) 18+ · Google Chrome · macOS 12+

**1. Build & run the app**

```bash
git clone https://github.com/vroy2008/amdion-computer.git
cd amdion-computer
npm install
npm run dev          # builds + launches Amdion (first build takes a few minutes)
```

An hourglass appears in your menu bar. Press **`⌘⇧Space`** to summon the panel.
(For a standalone `.app` instead of dev mode, run `npm run build` and find it under `src-tauri/target/release/bundle/`.)

**2. Connect Chrome** (the companion extension)

1. Open `chrome://extensions`
2. Turn on **Developer mode** (top-right)
3. Click **Load unpacked** and choose this repo's **`extension/`** folder

A welcome tab confirms **Connected to Amdion** once it links up — now friction, the Observer, and Read Mode work in your real Chrome.

**3. (Optional) Turn on chat**

The Gemini-powered assistant and voice transcription need a key; everything else works without one. Add it in **Settings → Advanced → AI key**, or drop a `.env` in the repo root:

```bash
echo "GEMINI_API_KEY=your_key_here" > .env
```

## Using it

- **Summon / dismiss** — `⌘⇧Space`, or click the menu-bar hourglass. Rebind the shortcut in **Settings → Advanced**.
- **Set your attention** — type what you're here to do; Amdion greets you with it next session.
- **Choose your friction** — **Settings → Attention**: **Off** just watches · **Soft** nudges you on distraction sites · **Lock-In** blocks them in Chrome.
- **Read Mode** — on any article press **`⌥⇧R`** (or click the quiet **READ** pill) for a calm full-screen reader; distractions lock for the length of the read, and `Esc` leaves.
- **Today** — the bar-chart icon opens your honest daily log: time on computer, per-app and per-site breakdown, reading time. It resets each day.

## Status

v1 is **built and verified locally** on macOS: the menu-bar front door + first-run Mac/Chrome tuning, the companion extension + configurable friction (live-confirmed over the localhost bridge), the sensing engine + "Today" Observer, Read Mode (reader + distraction-lock + reading stats), a rebindable global summon shortcut, and an in-app auto-updater.

The one remaining gate before a **one-click signed download** is Apple notarization (Developer ID). The release pipeline — signed universal DMG on GitHub Releases + auto-update — is built and CI-green; until enrolment lands, build from source as above. A locally-built app runs fine on your own machine.

> Detailed running log: [STATUS.md](STATUS.md) · vision & architecture: [docs/ROADMAP.md](docs/ROADMAP.md) · build plan: [docs/IMPLEMENTATION_PLAN.md](docs/IMPLEMENTATION_PLAN.md) · distribution: [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md).

The AI agent is deferred, but the architecture reserves a clean data/action surface for it.

## Architecture

- **Tauri v2** — Rust backend, native window management, macOS idle/active sensing, SQLite event store (~13 MB binary).
- **Chrome extension (MV3)** — declutters your real Chrome, reports tab activity, applies friction, hosts Read Mode. Talks to the app over a **localhost WebSocket**.
- **Gemini** (`reqwest`) — chat and audio transcription.
- **Vanilla frontend** — HTML/CSS/JS front door, settings, and Observer UI.

## Project structure

```
src-tauri/   # Tauri v2 Rust backend (app, sensing, db, bridge, gemini, commands)
frontend/    # Front door + settings + Observer UI, bridge.js adapter
extension/   # Chrome MV3 extension (declutter, friction, Read Mode, activity bridge)
docs/        # ROADMAP, IMPLEMENTATION_PLAN, DEPLOYMENT, product concept
```

---

*Built by [AMDION](https://amdion.org) — Time is your most valuable asset. We help you protect it.*
