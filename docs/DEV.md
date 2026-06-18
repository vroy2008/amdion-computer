# Development loop

You do **not** need to uninstall the app or remove + re-add the extension on every
change. That cycle was masking a different problem (see *The stale-instance trap*).

## The app

```bash
npm run dev          # auto-kills any stale Amdion, then runs `tauri dev`
```

- **Rust edits** (`src-tauri/`) — `tauri dev` rebuilds and relaunches automatically.
- **Frontend edits** (`frontend/`) — re-summon the panel (`⌃⇧A`) to pick them up.
- You never quit/relaunch by hand; just keep `npm run dev` running.

## The extension

After editing anything under `extension/`:

1. `chrome://extensions` → click the **↻ Reload** icon on the Amdion card.
2. For **content-script** edits (`content/*.js` — capture, reader, nudge), also
   **refresh the test tab** — already-injected tabs keep running old code until reloaded.

That's it. **Remove + Load unpacked is only needed if you change the manifest's
identity or move the folder** — not for normal code changes.

## When things act weird: the stale-instance trap

The app binds the **first free port** in `17872–17882`; the extension connects to
the **first port it finds**. So a leftover instance — a previous `tauri dev`, or the
installed `/Applications/AMDION.app` (same identifier, same port range, same
app-data dir) — steals the port, and the extension talks to **old code** while your
fresh build sits idle on the next port. Symptoms: captures/friction not working,
`[bridge] extension connected` on a build you didn't expect, two menu-bar hourglasses.

**The worst offender was launch-at-login.** `config.autostart` defaults to on, so
every launch re-registered the running binary as a LaunchAgent
(`~/Library/LaunchAgents/AMDION.plist`) — including the installed
`/Applications/AMDION.app`, which then auto-started at *every login* and grabbed the
port before any dev run. **Dev builds no longer register autostart** (a
`debug_assertions` gate in `src-tauri/src/lib.rs` actively clears any stale agent);
only release builds honor the setting. If you ever see that plist again, a release
build you launched created it — remove it with:

```bash
launchctl bootout "gui/$(id -u)/AMDION" 2>/dev/null; rm -f ~/Library/LaunchAgents/AMDION.plist
```

Fixes:

```bash
npm run dev:clean       # kill any running Amdion instance (also run automatically by `npm run dev`)
npm run dev:reset       # move app-data aside for a fresh start (Application Support only, in-place backup)
npm run dev:reset:hard  # full "first-launch" reset — see below
```

### `dev:reset` vs `dev:reset:hard`

`dev:reset` only moves `~/Library/Application Support/com.amdion.desktop` aside. But a
macOS **reinstall** leaves more behind — **`~/Library/Caches/...` and the WKWebView
store `~/Library/WebKit/...` survive too**, plus the launch-at-login agent and the
installed app. That leftover state is why a reinstall can still show *old stuff*.

`dev:reset:hard` clears the **complete** per-user surface for `com.amdion.desktop`
(Application Support + Caches + WebKit + the LaunchAgent + `/Applications/AMDION.app`),
moving everything to `~/.Trash` (timestamped, recoverable — nothing is deleted). It
**keeps `~/.amdion`** (the updater signing key — irreplaceable). Use it when you want a
genuine first-launch state, or before a clean reinstall.

Check for zombies / which port is live:

```bash
pgrep -xl AMDION; pgrep -xl amdion-computer
lsof -nP -iTCP -sTCP:LISTEN | grep 1787
cat "$HOME/Library/Application Support/com.amdion.desktop/bridge.json"   # the port the live app advertises
```

Tip: while developing, keep `/Applications/AMDION.app` quit (or remove it) so it
can't grab the bridge port behind your back.
