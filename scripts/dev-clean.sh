#!/bin/sh
# Amdion dev pre-flight — kill any prior *dev* instance so the fresh `tauri dev`
# build owns its bridge port (dev range 17883-17893) cleanly.
#
# Dev and release are now fully isolated: dev builds use a separate port range
# AND a separate app-data dir (`com.amdion.desktop.dev`) via a debug_assertions
# split (see bridge_ws.rs / config.rs), and the Chrome extension PREFERS the dev
# range (extension/background.js). So we deliberately leave the installed
# /Applications/AMDION.app running — dev and release coexist, and the extension
# talks to the dev build regardless. We only clear a *previous* `tauri dev`, since
# two dev instances would still collide on the dev range.
#
# IMPORTANT: both the dev binary and the installed app's executable are named
# `amdion-computer` (the bundle is AMDION.app, but its Mach-O is amdion-computer),
# so we match the dev instance by its PATH (`target/debug/...`), NOT by process
# name — a `pkill -x amdion-computer` would also kill the installed release app.
# No-op when no dev instance is running; always exits 0 so it can chain into
# `tauri dev`.

if pgrep -f 'target/debug/amdion-computer' >/dev/null 2>&1; then
  pkill -f 'target/debug/amdion-computer' >/dev/null 2>&1 || true
  echo "[dev-clean] stopped a previous dev instance"
  # Give the OS a moment to release the dev bridge port before tauri dev rebinds it.
  sleep 1
else
  echo "[dev-clean] no previous dev instance"
fi

exit 0
