#!/bin/sh
# Amdion dev — move the DEV app-data dir aside so the next `tauri dev` launch
# starts 100% fresh (re-runs onboarding, empty DB/notes). Nothing is deleted: the
# old dir (amdion.db*, config.json, notes/, bridge.json) is MOVED to a timestamped
# backup you can restore or remove later. Use when stale dev DB/config is causing
# issues, or to reproduce first-run.
#
# Targets the DEV identifier `com.amdion.desktop.dev` (dev builds use it via the
# debug_assertions split in config.rs) — NOT the installed release app's data, so
# this is safe to run while the release /Applications/AMDION.app is up.

DIR="$HOME/Library/Application Support/com.amdion.desktop.dev"

if [ ! -d "$DIR" ]; then
  echo "[reset] no dev app-data at $DIR (already clean)"
  exit 0
fi

# Refuse to run while a dev instance is up — it would just rewrite the files.
# Match the dev instance by PATH: the installed release app's executable is also
# named `amdion-computer`, but it uses a different data dir, so its running state
# is irrelevant here (no need to quit release to reset dev data).
if pgrep -f 'target/debug/amdion-computer' >/dev/null 2>&1; then
  echo "[reset] a dev instance is running — run 'npm run dev:clean' (or quit it) first."
  exit 1
fi

TS=$(date +%Y%m%d-%H%M%S)
BAK="${DIR}.bak-${TS}"
mv "$DIR" "$BAK"
echo "[reset] dev app-data moved to: $BAK"
echo "[reset] next dev launch starts fresh (onboarding re-runs)."
echo "[reset] restore everything with:  mv \"$BAK\" \"$DIR\""
echo "[reset] (for the AI key in dev, set GEMINI_API_KEY in .env — see docs/DEV.md)"
exit 0
