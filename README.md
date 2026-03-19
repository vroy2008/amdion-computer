# Amdion Computer

**An AI-powered desktop overlay that replaces the chaotic default computer experience with a calm, text-based interface.**

Think Olauncher for your computer — but smarter. A single surface you interact with: chat with it, ask it things, open apps through it, and it quietly protects your attention.

![Amdion Logo](amdion_logo_new.png)

## Features

- **Minimalist Text Launcher** — No icons, text-only favorites and search. Typography-driven UI.
- **AI Assistant** — Chat-based interaction powered by Gemini. Triage email, check calendar, launch apps — without entering the distraction zone.
- **Mindful Coach** — Subtle, well-timed nudges for context switches, long sessions, and drift detection.
- **Session Observer** — Passive activity logging with daily summaries, focus scores, and weekly trends.
- **Embedded Browser** — BrowserView for web apps, bypassing iframe restrictions. Inject custom CSS to strip distracting UI.
- **Smart Journaling** — Automatic screenshot capture and Gemini-powered summarization of daily activities.
- **Voice & Text Agent** — Speak or type commands to interact with your computer.

## Setup

```bash
# Install dependencies
npm install

# Create a .env file with your Gemini API key
echo "GEMINI_API_KEY=your_api_key_here" > .env

# Run the app
npm start
```

## Build (macOS)

```bash
# Using the build script
chmod +x build-dmg.sh
./build-dmg.sh

# Or via npm
npm run build
```

The packaged `.dmg` will be in the `dist/` directory.

## Project Structure

```
├── main.js            # Electron main process
├── preload.js         # Preload script (IPC bridge)
├── gemini.js          # Gemini AI integration
├── index.html         # Main UI
├── config.json        # App configuration (favorites, model settings)
├── observer-mvp/      # Observer pillar — session logging MVP
├── docs/              # Product documentation
└── build-dmg.sh       # macOS build script
```

## Architecture

- **Electron** — Cross-platform desktop framework with BrowserView, screen capture, and native window management.
- **Gemini API (`@google/genai`)** — Screenshot analysis, topic extraction, agent decision-making, and audio transcription.
- **Vanilla Frontend** — HTML, CSS, JS. Canvas API for visualizations.

## Documentation

See [docs/focused_computer.md](docs/focused_computer.md) for the full product concept.

---

*Built by [AMDION](https://amdion.org) — Time is your most valuable asset. We help you protect it.*
