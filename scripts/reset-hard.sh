#!/bin/sh
# Amdion dev — HARD reset to a genuine "first launch ever" state.
#
# Why this exists (vs. reset-data.sh): a macOS *reinstall* replaces
# /Applications/AMDION.app but leaves the whole per-user surface under ~/Library
# untouched — and reset-data.sh only moves the Application Support dir aside.
# Caches AND the WKWebView store (~/Library/WebKit) survive too, as does the
# launch-at-login agent. So the next install inherits old DB/config/cache/webview
# state — that's "old stuff on reinstall." This clears the COMPLETE surface for
# identifier com.amdion.desktop.
#
# Nothing is deleted: every item is MOVED to ~/.Trash (timestamped) so you can
# restore it. The updater signing key at ~/.amdion is KEPT — it is irreplaceable
# (see docs/DEPLOYMENT.md); losing it breaks updates for every existing install.

set -u
ID="com.amdion.desktop"
APP="/Applications/AMDION.app"
AGENT="$HOME/Library/LaunchAgents/AMDION.plist"
TRASH="$HOME/.Trash"
TS=$(date +%Y%m%d-%H%M%S)
moved=0

# trash <path> <unique-label> — move into Trash under a distinct name.
# The three Library dirs all share the basename "com.amdion.desktop", so a
# distinct label per call is required or they collide in Trash.
trash() {
  _src="$1"; _label="$2"
  [ -e "$_src" ] || return 0
  if mv "$_src" "$TRASH/${_label}.${TS}" 2>/dev/null; then
    echo "  trashed: $_src"
    moved=1
  else
    echo "  WARN: could not move $_src"
  fi
}

echo "[reset-hard] clearing every per-user trace of $ID ..."

# 1. Stop any running instance — it would just rewrite the files we move.
for name in AMDION amdion-computer; do
  if pgrep -x "$name" >/dev/null 2>&1; then
    pkill -x "$name" >/dev/null 2>&1 || true
    echo "  stopped: $name"
  fi
done
sleep 1

# 2. Launch-at-login agent — the RunAtLoad zombie that grabs the bridge port
#    before any dev run (see docs/DEV.md). Unload before trashing.
launchctl bootout "gui/$(id -u)/AMDION" >/dev/null 2>&1 || true
launchctl unload "$AGENT" >/dev/null 2>&1 || true
trash "$AGENT" "AMDION.plist"

# 3. The installed app — shares the identifier, port range and app-data dir with
#    dev builds, so leaving it around reintroduces the stale-instance trap.
trash "$APP" "AMDION.app"

# 4. Per-user data that survives a reinstall (the actual "old stuff").
trash "$HOME/Library/Application Support/$ID"               "AppSupport-$ID"
trash "$HOME/Library/Caches/$ID"                            "Caches-$ID"
trash "$HOME/Library/WebKit/$ID"                            "WebKit-$ID"
# Best-effort: usually absent for this app, but clear them if a build created them.
trash "$HOME/Library/HTTPStorages/$ID"                      "HTTPStorages-$ID"
trash "$HOME/Library/Saved Application State/${ID}.savedState" "SavedState-$ID"
trash "$HOME/Library/Preferences/${ID}.plist"              "Prefs-$ID"

echo ""
if [ "$moved" = "1" ]; then
  echo "[reset-hard] done — next launch is genuinely first-run."
  echo "[reset-hard] everything moved to $TRASH (restore from there if needed)."
else
  echo "[reset-hard] nothing to clear — already pristine."
fi
echo "[reset-hard] KEPT ~/.amdion (updater signing key — irreplaceable, never wiped)."
exit 0
