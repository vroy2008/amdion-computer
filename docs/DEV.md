# Development loop

You do **not** need to uninstall the app or remove + re-add the extension on every
change. Dev and release are now fully isolated and run side by side.

## The app

```bash
npm run dev          # stops any previous `tauri dev`, then runs `tauri dev`
```

- **Rust edits** (`src-tauri/`) — `tauri dev` rebuilds and relaunches automatically.
- **Frontend edits** (`frontend/`) — re-summon the panel (`⌃⇧A`) to pick them up.
- You never quit/relaunch by hand; just keep `npm run dev` running.

The installed `/Applications/AMDION.app` can stay running the whole time — `npm run
dev` no longer kills it (see *Dev and release are isolated*, below).

## The extension

After editing anything under `extension/`:

1. `chrome://extensions` → click the **↻ Reload** icon on the Amdion card.
2. For **content-script** edits (under `core/` and `features/*/` — nudge, reader, capture), also
   **refresh the test tab** — already-injected tabs keep running old code until reloaded.

That's it. **Remove + Load unpacked is only needed if you change the manifest's
identity or move the folder** — not for normal code changes.

## Dev and release are isolated (the old stale-instance trap, fixed)

Dev (`tauri dev`) and release (`/Applications/AMDION.app`) used to share a bundle
identifier, a bridge port range, and an app-data dir, so they fought over the port
and the extension latched onto whichever instance won the race — often the stale
one. Now they're split by build profile (`#[cfg(debug_assertions)]`: `tauri dev` is
a debug build, the `tauri build` release bundle is not):

| | Release (`/Applications/AMDION.app`) | Dev (`tauri dev`) |
|---|---|---|
| Bundle id / app-data dir | `com.amdion.desktop` | `com.amdion.desktop.dev` |
| Bridge port range | 17872–17882 | 17883–17893 |

The Chrome extension scans the **dev range first**, then release (`extension/
background.js`), so a running `tauri dev` build always wins. If the extension is
parked on the release app and you then start `tauri dev`, the keepalive re-probes
the dev range and **migrates automatically** within a tick (~24s); an extension
**Reload** is the instant path.

Because dev has its own app-data, a fresh `tauri dev` build runs onboarding off its
own `onboardingComplete` flag — independent of the installed app, so onboarding
shows reliably. Two menu-bar hourglasses (release + dev) is now **expected and
fine**, not a symptom.

**Dev's AI key is separate too.** The release Settings key lives in the release
app-data and does not carry into dev. For dev, put `GEMINI_API_KEY=…` in a repo
`.env` (loaded by `dotenvy` at startup — see `config.rs`) or paste a key into dev's
own onboarding/Settings.

### The launch-at-login zombie (still worth knowing)

`config.autostart` defaults on, so a **release** launch registers a LaunchAgent
(`~/Library/LaunchAgents/AMDION.plist`) that relaunches the release app at every
login. **Dev builds never register autostart** (a `debug_assertions` gate in
`src-tauri/src/lib.rs` actively clears any stale agent). If you ever see that plist,
a release build you launched created it; remove it with:

```bash
launchctl bootout "gui/$(id -u)/AMDION" 2>/dev/null; rm -f ~/Library/LaunchAgents/AMDION.plist
```

## Fresh-start helpers

```bash
npm run dev:clean       # stop a previous `tauri dev` (also run automatically by `npm run dev`)
npm run dev:reset       # move the DEV app-data aside for a fresh first-run (in-place backup)
npm run dev:reset:hard  # full "first-launch" reset — see below
```

### `dev:reset` vs `dev:reset:hard`

`dev:reset` moves the **dev** dir (`~/Library/Application Support/com.amdion.desktop.dev`)
aside — the quickest way to replay first-run onboarding on a dev build. It leaves
the installed release app and its data untouched, so it's safe to run while release
is up.

`dev:reset:hard` simulates a genuine clean reinstall: it clears the **complete**
per-user surface (Application Support + Caches + WebKit + saved state) for **both**
identifiers, plus the installed `/Applications/AMDION.app` and the release
LaunchAgent — everything moved to `~/.Trash` (timestamped, recoverable, nothing
deleted). It **keeps `~/.amdion`** (the updater signing key — irreplaceable). Use it
for a true first-launch state, or before a clean reinstall.

## Check which instance is live / which port

```bash
# Both the installed app and `tauri dev` run an executable named amdion-computer
# (the bundle is AMDION.app, its Mach-O is amdion-computer) — tell them apart by PATH:
pgrep -fl amdion-computer                              # /Applications/… = release, target/debug/… = dev
lsof -nP -iTCP -sTCP:LISTEN | grep 1787                # 17872… = release, 17883… = dev
cat "$HOME/Library/Application Support/com.amdion.desktop/bridge.json"      # release port
cat "$HOME/Library/Application Support/com.amdion.desktop.dev/bridge.json"  # dev port
```
