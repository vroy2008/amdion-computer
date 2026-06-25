// Mac tuning commands.
//
// Each tweak is modeled as a typed, reversible unit:
//   { id, label, description, kind: scriptable | walkthrough, state }
// so a future agent can call `apply_mac_tuning` / `revert_mac_tuning` itself.
//
// - `scriptable` tweaks run a vetted `defaults write` (+ optional `killall`).
//   Their `state` is read back from `defaults read` so the UI shows before/after.
// - `walkthrough` tweaks can't be set safely from the CLI; Amdion opens the
//   relevant System Settings pane and shows the steps (overlay-and-point).
//
// Part of the first-run "Tune your Mac" layer.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TweakKind {
    Scriptable,
    Walkthrough,
}

/// Internal, static description of one tuning tweak.
struct TweakSpec {
    id: &'static str,
    label: &'static str,
    description: &'static str,
    kind: TweakKind,
    // Scriptable fields ----------------------------------------------------
    domain: &'static str,
    key: &'static str,
    /// Type flag for `defaults write` (e.g. "-bool", "-int").
    value_type: &'static str,
    /// Value written when the tweak is applied (the focus-friendly state).
    on_value: &'static str,
    /// Value written when the tweak is reverted (the macOS default).
    off_value: &'static str,
    /// What `defaults read` prints when the tweak is applied (normalized).
    read_on: &'static str,
    /// App to `killall` after writing so the change takes effect.
    killall: Option<&'static str>,
    // Walkthrough fields ---------------------------------------------------
    /// `x-apple.systempreferences:` URL for the relevant pane.
    pane: &'static str,
    /// Ordered steps shown in the overlay-and-point UI.
    steps: &'static [&'static str],
}

const fn scriptable(
    id: &'static str,
    label: &'static str,
    description: &'static str,
    domain: &'static str,
    key: &'static str,
    value_type: &'static str,
    on_value: &'static str,
    off_value: &'static str,
    read_on: &'static str,
    killall: Option<&'static str>,
) -> TweakSpec {
    TweakSpec {
        id,
        label,
        description,
        kind: TweakKind::Scriptable,
        domain,
        key,
        value_type,
        on_value,
        off_value,
        read_on,
        killall,
        pane: "",
        steps: &[],
    }
}

const fn walkthrough(
    id: &'static str,
    label: &'static str,
    description: &'static str,
    pane: &'static str,
    steps: &'static [&'static str],
) -> TweakSpec {
    TweakSpec {
        id,
        label,
        description,
        kind: TweakKind::Walkthrough,
        domain: "",
        key: "",
        value_type: "",
        on_value: "",
        off_value: "",
        read_on: "",
        killall: None,
        pane,
        steps,
    }
}

/// The vetted tweak registry. Order here is the order shown in the UI.
const TWEAKS: &[TweakSpec] = &[
    scriptable(
        "dock-autohide",
        "Auto-hide the Dock",
        "Keeps the Dock out of sight until you reach for it.",
        "com.apple.dock",
        "autohide",
        "-bool",
        "true",
        "false",
        "1",
        Some("Dock"),
    ),
    scriptable(
        "dock-recents",
        "Hide recent apps in Dock",
        "Removes the auto-populated recent-apps section from the Dock.",
        "com.apple.dock",
        "show-recents",
        "-bool",
        "false",
        "true",
        "0",
        Some("Dock"),
    ),
    scriptable(
        "finder-desktop-icons",
        "Hide desktop icons",
        "Hides files and folders on the desktop for a clean backdrop.",
        "com.apple.finder",
        "CreateDesktop",
        "-bool",
        "false",
        "true",
        "0",
        Some("Finder"),
    ),
    scriptable(
        "reduce-transparency",
        "Reduce transparency",
        "Flattens translucent menus and backgrounds to cut visual noise.",
        "com.apple.universalaccess",
        "reduceTransparency",
        "-bool",
        "true",
        "false",
        "1",
        None,
    ),
    scriptable(
        "reduce-motion",
        "Reduce motion",
        "Disables window and Space animations.",
        "com.apple.universalaccess",
        "reduceMotion",
        "-bool",
        "true",
        "false",
        "1",
        None,
    ),
    walkthrough(
        "notifications",
        "Quiet notifications",
        "Turn off banners and badges for distracting apps.",
        "x-apple.systempreferences:com.apple.Notifications-Settings.extension",
        &[
            "Pick the apps that interrupt you most",
            "Turn \"Allow Notifications\" off, or switch the alert style to None",
            "Disable badges and sounds for anything non-essential",
        ],
    ),
    walkthrough(
        "focus",
        "Set up a Focus mode",
        "Create a Focus that silences everything except what you choose.",
        "x-apple.systempreferences:com.apple.Focus-Settings.extension",
        &[
            "Add a Focus (e.g. \"Work\")",
            "Allow only the people and apps you actually need",
            "Optionally schedule it to turn on automatically",
        ],
    ),
    walkthrough(
        "spotlight",
        "Trim Spotlight & Siri suggestions",
        "Stop Spotlight from surfacing news, trending, and web suggestions.",
        "x-apple.systempreferences:com.apple.Spotlight-Settings.extension",
        &[
            "Uncheck Siri Suggestions and any web/trending categories",
            "Keep only the result types you search for",
        ],
    ),
];

fn spec(id: &str) -> Option<&'static TweakSpec> {
    TWEAKS.iter().find(|t| t.id == id)
}

/// The few tweaks surfaced in the (deliberately minimal) UI — the highest-impact
/// "quieter desktop" wins: hide the Dock, clear the desktop, quiet notifications.
/// The rest of `TWEAKS` stays available to the agent via `list_mac_tweaks`.
const FEATURED_IDS: &[&str] = &["dock-autohide", "finder-desktop-icons", "notifications"];

/// Public, agent-readable view of one tweak.
#[derive(Debug, Clone, Serialize)]
pub struct MacTweak {
    pub id: String,
    pub label: String,
    pub description: String,
    pub kind: TweakKind,
    /// Whether this tweak is surfaced in the minimal UI (see `FEATURED_IDS`).
    pub featured: bool,
    /// For scriptable tweaks: whether it's currently in the focus-friendly
    /// state. `None` for walkthrough tweaks (state can't be read reliably).
    pub enabled: Option<bool>,
    /// Walkthrough-only: the System Settings pane and the steps to follow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<String>,
}

/// Result of applying or reverting a single tweak.
#[derive(Debug, Clone, Serialize)]
pub struct TweakResult {
    pub id: String,
    pub ok: bool,
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Read the current value of a scriptable tweak via `defaults read`.
/// Returns `Some(true)` if it matches the focus-friendly state.
fn read_enabled(t: &TweakSpec) -> Option<bool> {
    let out = Command::new("defaults")
        .args(["read", t.domain, t.key])
        .output()
        .ok()?;
    if !out.status.success() {
        // Key not set yet → macOS default, i.e. not applied.
        return Some(false);
    }
    let val = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Some(val == t.read_on)
}

fn to_view(t: &TweakSpec) -> MacTweak {
    match t.kind {
        TweakKind::Scriptable => MacTweak {
            id: t.id.into(),
            label: t.label.into(),
            description: t.description.into(),
            kind: t.kind,
            featured: FEATURED_IDS.contains(&t.id),
            enabled: read_enabled(t),
            pane: None,
            steps: Vec::new(),
        },
        TweakKind::Walkthrough => MacTweak {
            id: t.id.into(),
            label: t.label.into(),
            description: t.description.into(),
            kind: t.kind,
            featured: FEATURED_IDS.contains(&t.id),
            enabled: None,
            pane: Some(t.pane.into()),
            steps: t.steps.iter().map(|s| s.to_string()).collect(),
        },
    }
}

// ── Original-value snapshots ──────────────────────────────────────────────
//
// Revert must restore the user's PRIOR value, not a guessed default — otherwise
// applying then reverting silently rewrites a setting the user had customized
// (e.g. Reduce Transparency they'd turned on for accessibility). So the first
// time a tweak is applied we snapshot its current `defaults read` value (or
// `None` if the key was unset), and revert restores exactly that.

fn snapshots_path() -> PathBuf {
    crate::config::app_data_dir().join("tuning_snapshots.json")
}

fn read_snapshots() -> HashMap<String, Option<String>> {
    fs::read_to_string(snapshots_path())
        .ok()
        .and_then(|d| serde_json::from_str(&d).ok())
        .unwrap_or_default()
}

fn write_snapshots(map: &HashMap<String, Option<String>>) {
    if let Ok(json) = serde_json::to_string_pretty(map) {
        let _ = fs::write(snapshots_path(), json);
    }
}

/// Current raw `defaults read` value, or `None` if the key isn't set.
fn current_raw(t: &TweakSpec) -> Option<String> {
    let out = Command::new("defaults")
        .args(["read", t.domain, t.key])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn refresh(t: &TweakSpec) {
    if let Some(app) = t.killall {
        // `killall` failing (e.g. app not running) is non-fatal.
        let _ = Command::new("killall").arg(app).status();
    }
}

fn defaults_write(t: &TweakSpec, value: &str) -> Result<(), String> {
    let status = Command::new("defaults")
        .args(["write", t.domain, t.key, t.value_type, value])
        .status()
        .map_err(|e| format!("failed to run `defaults`: {e}"))?;
    if !status.success() {
        return Err(format!("`defaults write {} {}` failed", t.domain, t.key));
    }
    refresh(t);
    Ok(())
}

fn defaults_delete(t: &TweakSpec) -> Result<(), String> {
    // Deleting an already-absent key returns non-zero — that's fine.
    let _ = Command::new("defaults")
        .args(["delete", t.domain, t.key])
        .status();
    refresh(t);
    Ok(())
}

fn apply_tweak(t: &TweakSpec, enable: bool) -> Result<(), String> {
    if enable {
        // Snapshot the original value once, before our first overwrite.
        let mut snaps = read_snapshots();
        if !snaps.contains_key(t.id) {
            snaps.insert(t.id.to_string(), current_raw(t));
            write_snapshots(&snaps);
        }
        defaults_write(t, t.on_value)
    } else {
        // Restore the snapshot if we have one; else fall back to the default.
        let mut snaps = read_snapshots();
        match snaps.remove(t.id) {
            Some(orig) => {
                write_snapshots(&snaps);
                match orig {
                    Some(v) => defaults_write(t, &v),
                    None => defaults_delete(t),
                }
            }
            None => defaults_write(t, t.off_value),
        }
    }
}

fn apply_one(id: &str, enable: bool) -> TweakResult {
    match spec(id) {
        None => TweakResult {
            id: id.into(),
            ok: false,
            enabled: None,
            error: Some("unknown tweak id".into()),
        },
        Some(t) if t.kind != TweakKind::Scriptable => TweakResult {
            id: id.into(),
            ok: false,
            enabled: None,
            error: Some("tweak is walkthrough-only; open its settings pane".into()),
        },
        Some(t) => match apply_tweak(t, enable) {
            Ok(()) => TweakResult {
                id: id.into(),
                ok: true,
                enabled: read_enabled(t),
                error: None,
            },
            Err(e) => TweakResult {
                id: id.into(),
                ok: false,
                enabled: read_enabled(t),
                error: Some(e),
            },
        },
    }
}

// ── Commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_mac_tweaks() -> Result<Vec<MacTweak>, String> {
    Ok(TWEAKS.iter().map(to_view).collect())
}

#[tauri::command]
pub fn apply_mac_tuning(keys: Vec<String>) -> Result<Vec<TweakResult>, String> {
    Ok(keys.iter().map(|k| apply_one(k, true)).collect())
}

#[tauri::command]
pub fn revert_mac_tuning(keys: Vec<String>) -> Result<Vec<TweakResult>, String> {
    Ok(keys.iter().map(|k| apply_one(k, false)).collect())
}

/// Open a System Settings pane for a walkthrough tweak (or any `x-apple...` URL).
#[tauri::command]
pub fn open_settings_pane(pane: String) -> Result<(), String> {
    if !pane.starts_with("x-apple.systempreferences:") {
        return Err("refusing to open a non-System-Settings URL".into());
    }
    Command::new("open")
        .arg(&pane)
        .status()
        .map_err(|e| format!("failed to open settings pane: {e}"))?;
    Ok(())
}
