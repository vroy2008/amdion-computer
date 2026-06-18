#!/bin/sh
# Amdion dev — move the app-data dir aside so the next launch starts 100% fresh.
# Nothing is deleted: the old dir (amdion.db*, config.json, notes/, bridge.json)
# is MOVED to a timestamped backup you can restore or remove later. Use when a
# stale DB or config from an older build is causing issues.

DIR="$HOME/Library/Application Support/com.amdion.desktop"

if [ ! -d "$DIR" ]; then
  echo "[reset] no app-data at $DIR (already clean)"
  exit 0
fi

# Refuse to run while the app is up — it would just rewrite the files.
if pgrep -x AMDION >/dev/null 2>&1 || pgrep -x amdion-computer >/dev/null 2>&1; then
  echo "[reset] Amdion is running — run 'npm run dev:clean' (or quit it) first."
  exit 1
fi

TS=$(date +%Y%m%d-%H%M%S)
BAK="${DIR}.bak-${TS}"
mv "$DIR" "$BAK"
echo "[reset] app-data moved to: $BAK"
echo "[reset] next launch starts fresh."
echo "[reset] restore everything with:  mv \"$BAK\" \"$DIR\""
echo "[reset] (your AI key lives in the backup's config.json — re-add it in Settings or copy it back)"
exit 0
