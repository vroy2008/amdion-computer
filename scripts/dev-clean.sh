#!/bin/sh
# Amdion dev pre-flight — kill any running Amdion instance so the fresh dev build
# owns bridge port 17872 and the Chrome extension can't latch onto a zombie app.
#
# Why this exists: the app binds the FIRST free port in 17872-17882 and the
# extension connects to the FIRST one it finds (see bridge_ws.rs / background.js).
# A leftover instance — a previous `tauri dev`, or the installed
# /Applications/AMDION.app, which shares the same identifier — steals the port,
# so the extension ends up talking to old code while your new build sits idle.
# Killing stale instances first makes "edit -> npm run dev" deterministic.
# No-op when nothing is running. Always exits 0 so it can chain into `tauri dev`.

# AMDION         = the bundled / installed app binary (productName)
# amdion-computer = the dev binary tauri dev runs from target/debug
killed=0
for name in AMDION amdion-computer; do
  if pgrep -x "$name" >/dev/null 2>&1; then
    pkill -x "$name" >/dev/null 2>&1 || true
    killed=1
  fi
done

if [ "$killed" = "1" ]; then
  echo "[dev-clean] stopped a running Amdion instance"
  # Give the OS a moment to release the bridge port before tauri dev rebinds it.
  sleep 1
else
  echo "[dev-clean] no running Amdion instance"
fi

exit 0
