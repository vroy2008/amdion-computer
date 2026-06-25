<p align="center">
  <img src="amdion_logo_new.png" alt="Amdion" width="116">
</p>

<h1 align="center">Amdion</h1>

<p align="center"><b>A calm layer for your attention, on your Mac.</b></p>

Amdion helps you *see* where your attention goes on your computer and gently steer it back — so you're driving, not being driven. Everything stays on your device.

It isn't an operating system or a browser you live inside. It's a quiet front door you summon from the menu bar, plus a companion extension for the Chrome you already use.

---

## What it does

Amdion has two parts that talk to each other on your machine.

**A menu-bar app — it keeps an honest record of your time.**
Summon it with `⌃⇧A`, tell it what you're here to do, and it quietly tracks how your time actually goes. Review it in **Today**, or export to CSV/JSON. No Dock icon, no window to manage.

**A Chrome extension — it helps with the distracting sites.**
It always tracks your browsing in the background, and applies exactly the amount of friction you choose on sites like YouTube, X, Reddit, and Instagram:

| Mode | What happens |
|------|--------------|
| **Off** | Just watches. |
| **Nudge** | A quiet reminder card appears on distracting sites. |
| **Block** | Distracting sites redirect to a calm blank page. |

You can set the mode yourself from the extension's toolbar button — or let it follow what you told the app you're doing:

| When you're here to… | …it switches to |
|----------------------|-----------------|
| Do deep work | **Block** |
| Communicate | **Nudge** |
| Explore | **Off** |

Your real Chrome — logins, profile, and other extensions — stays untouched.

---

## Get started

Amdion runs on **macOS + Chrome**. There's no one-click download yet, so you build it from source. It takes about five minutes, mostly the first Rust compile. (A locally-built app isn't quarantined, so macOS won't block it.)

> **Using an AI coding assistant?** Point it at this repo and say: *"Set up and run Amdion on my Mac — install anything missing, build the app, then help me load the Chrome extension."*

**You'll need:** [Rust](https://rustup.rs) · [Node.js 18+](https://nodejs.org) · Google Chrome · macOS 10.15+

**1. Build and run the app**

```bash
git clone https://github.com/vroy2008/amdion-computer.git
cd amdion-computer
npm install
npm run dev          # builds and launches Amdion (first build takes a few minutes)
```

An hourglass appears in your menu bar, and a short first-run setup walks you through a couple of optional Mac tweaks and connecting Chrome. Press **`⌃⇧A`** to open the panel any time.

*(For a standalone app to keep, run `npm run build` — you'll find it at `src-tauri/target/release/bundle/macos/AMDION.app`.)*

**2. Connect Chrome**

1. Open `chrome://extensions`
2. Turn on **Developer mode** (top-right)
3. Click **Load unpacked** and choose this repo's **`extension/`** folder

The extension's popup shows **Connected** once it links to the app. That's it.

---

## Day to day

- **Open / close** — `⌃⇧A`, or click the menu-bar hourglass. `Esc` closes.
- **Set what you're doing** — pick **Deep work · Communicate · Explore**, or write your own. It's saved to your log, and when Chrome is connected it sets the extension's mode for you.
- **Or set the friction directly** — open the extension button and choose **Off · Nudge · Block**.
- **Tune your Mac** — a few reversible tweaks for a quieter desktop, in the panel under **Tune your Mac**.
- **Today** — your honest daily log: time on the computer, a breakdown by app and site, and your sessions. Export it to CSV/JSON. Resets each day; never leaves your device.

---

## What else is in here

The repo also carries some extra, experimental features — an in-page reader, quick notes, a "present" mode, page-decluttering, and an optional AI assistant. **They're all off by default.** A fresh install runs only the time tracking and the Off / Nudge / Block modes above; the extras stay dormant until you turn them on, and the assistant isn't even compiled into the default build.

---

## Project status

The core is built, working on macOS, and used day to day. It's **build-from-source** for now — aimed at developers and early adopters — while a signed one-click download and a Chrome Web Store listing are still ahead. The main piece being hardened is a full end-to-end test of the Chrome modes in a real browser.

More detail: [vision & architecture](docs/ROADMAP.md) · [distribution](docs/DEPLOYMENT.md) · [dev notes](docs/DEV.md)

---

## How it's built

- **Tauri 2 (Rust)** — the menu-bar app: native window, macOS activity/idle sensing, a local SQLite store, and a localhost link to the extension.
- **Chrome extension (MV3)** — `core/` (tracking, nudge, block) talks to the app over localhost. The extra features live in `features/` behind a switch that defaults off.
- **Plain HTML/CSS/JS** — the front door and the Today view.

```
src-tauri/   # Rust backend (app, sensing, database, bridge)
frontend/    # Front door + Today UI
extension/   # Chrome extension — core/ + optional features/
docs/        # Architecture, distribution, dev notes
```

---

## License

MIT — © 2026 AMDION. See [LICENSE](LICENSE) and [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md). The in-page reader bundles Mozilla's [Readability](https://github.com/mozilla/readability) (Apache-2.0), and the app is built on [Tauri](https://tauri.app).

The **AMDION** name and logo are the author's brand — the MIT license covers the code, not the brand, so forks should rename.

---

<p align="center"><i>Built by <a href="https://amdion.org">AMDION</a> — your time is your most valuable asset; we help you protect it.</i></p>
