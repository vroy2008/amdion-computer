// Config: types and JSON file persistence.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(rename = "apiKey", default)]
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,

    // ── Behavior (Step 1) ──
    /// "off" | "soft" | "lockin". Wired to the extension in Step 2.
    #[serde(rename = "frictionLevel", default = "default_friction")]
    pub friction_level: String,
    /// Offer to record an intent at the start of each new session.
    #[serde(rename = "greetingEnabled", default = "default_true")]
    pub greeting_enabled: bool,
    /// Set once the first-run onboarding flow has been completed.
    #[serde(rename = "onboardingComplete", default)]
    pub onboarding_complete: bool,
    /// Idle minutes that count as a break (advanced; used by Step 3 sensing).
    #[serde(rename = "breakThresholdMins", default = "default_break_threshold")]
    pub break_threshold_mins: u32,
    /// Idle minutes that close a session (advanced; used by Step 3 sensing).
    #[serde(rename = "sessionGapMins", default = "default_session_gap")]
    pub session_gap_mins: u32,

    // ── Response / friction (Step 2) ──
    /// User-added domains the friction layer acts on, on top of the extension's
    /// built-in distraction list. Soft = nudge; Lock-In = block/redirect.
    #[serde(rename = "blockList", default)]
    pub block_list: Vec<String>,

    // ── App behavior ──
    /// Launch Amdion automatically at login. On by default; user-disableable.
    #[serde(default = "default_true")]
    pub autostart: bool,

    // ── Read Mode ──
    /// Reading-surface preferences, mirrored to the extension's reader.
    #[serde(default)]
    pub reading: ReadingPrefs,

    // ── Reshape (Phase 2) ──
    /// Per-site "calm the trap" reshaping — declutter + feed-fade + the in-page
    /// behavioral nudges. A switch independent of the friction level: a site can
    /// be calmed even in Off mode (see docs/REORIENTATION.md §9). Mirrored to the
    /// extension's chrome.storage.local "reshape".
    #[serde(default)]
    pub reshape: ReshapeConfig,

    // ── Hotkeys ──
    /// Global "summon the panel" accelerator, in Tauri global-shortcut syntax
    /// (e.g. "Control+Shift+A"). Rebindable in Settings → Advanced;
    /// applied live and re-registered on change (see commands/shortcut.rs).
    #[serde(rename = "summonShortcut", default = "default_summon_shortcut")]
    pub summon_shortcut: String,
}

/// Read Mode preferences. Pushed to the extension (chrome.storage.local
/// "reading"), which content/reader.js reads and live-applies. The in-reader
/// controls can override per session; this is the panel-managed default.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingPrefs {
    /// "sepia" | "light" | "dark".
    #[serde(default = "default_theme")]
    pub theme: String,
    /// "serif" | "sans".
    #[serde(default = "default_typeface")]
    pub typeface: String,
    /// Font-size step, 1..=5.
    #[serde(default = "default_read_size")]
    pub size: u8,
    /// Words-per-minute for the "N min read" estimate.
    #[serde(default = "default_wpm")]
    pub wpm: u32,
    /// Show the quiet in-page "Read" pill on article-like pages.
    #[serde(rename = "pillEnabled", default = "default_true")]
    pub pill_enabled: bool,
    /// Offer a one-tap "Present" (fullscreen + the wrap) on long non-article
    /// pages once reading/work has visibly settled. Opt-in; default OFF — the
    /// offer is ambient but never auto-engages (see docs/REORIENTATION.md §9).
    #[serde(rename = "presentOffer", default)]
    pub present_offer: bool,
    /// "The wrap": block your distraction sites in Chrome for the duration of a
    /// read, then restore your normal friction level on exit. Snapshot/restore
    /// is implicit — the extension layers a Lock-In over your base level while
    /// reading and drops back to it after (see extension/background.js).
    #[serde(rename = "lockTabs", default = "default_true")]
    pub lock_tabs: bool,
    /// Optional macOS Shortcut to run when a read *starts* (e.g. one that turns
    /// on a Focus). Empty = do nothing. App-side only — not sent to the
    /// extension; run via `shortcuts run` (commands/read.rs).
    #[serde(rename = "focusShortcutStart", default)]
    pub focus_shortcut_start: String,
    /// Optional macOS Shortcut to run when a read *ends* (e.g. one that turns the
    /// Focus back off). Empty = do nothing.
    #[serde(rename = "focusShortcutEnd", default)]
    pub focus_shortcut_end: String,
}

/// Reshape preferences. Governs all in-page "calm the trap" behaviour — the
/// `declutter.css` decorations, feed-fade, and the behavioral nudges
/// (over-scroll / redirect-chase / idle-return / YouTube drift). Pushed to the
/// extension (chrome.storage.local "reshape"); content/reshape.js applies the
/// `html.amdion-reshape` gate every reshaping item keys off.
///
/// Default-on for the known trap sites (so there's no regression from the
/// always-on declutter that shipped) via an *opt-out* `disabled_sites` list:
/// a site absent from the list is reshaped. The aggressive feed-hiding items
/// (`feed_fade`, `hide_youtube_home`) default OFF and are opt-in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReshapeConfig {
    /// Master switch for all reshaping. On by default; off disables every
    /// in-page decoration and behavioral nudge at once (the global escape hatch).
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Domains the user has explicitly turned reshaping OFF for. Stored as an
    /// opt-out set so a known trap site stays calmed by default and new built-in
    /// sites need no migration.
    #[serde(rename = "disabledSites", default)]
    pub disabled_sites: Vec<String>,
    /// Feed-fade: opacity-fade the bottomless feed past the fold (X / LinkedIn).
    /// Experiment-tier; default OFF (aggressive feed-hiding is opt-in, §6).
    #[serde(rename = "feedFade", default)]
    pub feed_fade: bool,
    /// Hide the YouTube algorithmic home grid (search / Subscriptions stay).
    /// Opt-in; default OFF.
    #[serde(rename = "hideYoutubeHome", default)]
    pub hide_youtube_home: bool,
}

impl Default for ReshapeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            disabled_sites: Vec::new(),
            feed_fade: false,
            hide_youtube_home: false,
        }
    }
}

impl Default for ReadingPrefs {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            typeface: default_typeface(),
            size: default_read_size(),
            wpm: default_wpm(),
            pill_enabled: true,
            present_offer: false,
            lock_tabs: true,
            focus_shortcut_start: String::new(),
            focus_shortcut_end: String::new(),
        }
    }
}

fn default_model() -> String {
    // The assistant is an off-by-default feature (see Cargo `assistant`); no
    // model id ships by default. Set one when enabling the assistant.
    String::new()
}

fn default_friction() -> String {
    "soft".to_string()
}

/// Built-in summon shortcut — the single source of truth is `SUMMON_SHORTCUT`
/// in lib.rs, which is also the registration fallback if a saved binding won't
/// take.
pub fn default_summon_shortcut() -> String {
    crate::SUMMON_SHORTCUT.to_string()
}

fn default_true() -> bool {
    true
}

fn default_break_threshold() -> u32 {
    5
}

fn default_session_gap() -> u32 {
    30
}

fn default_theme() -> String {
    "sepia".to_string()
}

fn default_typeface() -> String {
    "serif".to_string()
}

fn default_read_size() -> u8 {
    3
}

fn default_wpm() -> u32 {
    240
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: default_model(),
            friction_level: default_friction(),
            greeting_enabled: true,
            onboarding_complete: false,
            break_threshold_mins: default_break_threshold(),
            session_gap_mins: default_session_gap(),
            block_list: Vec::new(),
            autostart: true,
            reading: ReadingPrefs::default(),
            reshape: ReshapeConfig::default(),
            summon_shortcut: default_summon_shortcut(),
        }
    }
}

/// macOS bundle identifier. Release matches `tauri.conf.json`; dev builds get a
/// `.dev` suffix so `tauri dev` and the installed release app keep SEPARATE
/// app-data dirs (config + onboarding state, db, notes, bridge.json, tuning) and
/// never clobber each other — a fresh dev run then reliably shows onboarding,
/// independent of release. Split by build profile via `debug_assertions`
/// (`tauri dev` = debug, the `tauri build` release bundle = not), the same gate
/// lib.rs uses for autostart/updater. (A `tauri build --debug` artifact would
/// carry the `.dev` identity too — intended: a debug bundle is a dev artifact;
/// the real /Applications release is always `tauri build`.)
#[cfg(not(debug_assertions))]
const APP_IDENTIFIER: &str = "com.amdion.desktop";
#[cfg(debug_assertions)]
const APP_IDENTIFIER: &str = "com.amdion.desktop.dev";

/// Amdion's per-user data directory:
/// `~/Library/Application Support/<APP_IDENTIFIER>` (release `com.amdion.desktop`;
/// dev `com.amdion.desktop.dev`), created on first use.
///
/// Config and tuning snapshots live here, not next to the executable: once
/// Amdion is installed in `/Applications` its bundle is read-only, so the old
/// exe-adjacent path silently failed to persist. Shared with `tuning.rs`.
pub fn app_data_dir() -> PathBuf {
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| {
            home.join("Library")
                .join("Application Support")
                .join(APP_IDENTIFIER)
        })
        .unwrap_or_else(|| PathBuf::from("."));
    let _ = fs::create_dir_all(&base);
    base
}

fn config_path() -> PathBuf {
    app_data_dir().join("config.json")
}

pub fn read_config() -> AppConfig {
    let path = config_path();

    // Start from the saved file when present (serde `default`s fill any new
    // fields a stored config predates; unknown old fields are ignored),
    // otherwise from built-in defaults.
    let mut config = path
        .exists()
        .then(|| fs::read_to_string(&path).ok())
        .flatten()
        .and_then(|data| serde_json::from_str::<AppConfig>(&data).ok())
        .unwrap_or_default();

    // An env-provided API key wins (handy for dev without touching the file).
    if let Ok(key) = std::env::var("GEMINI_API_KEY") {
        if !key.is_empty() {
            config.api_key = key;
        }
    }
    config
}

pub fn write_config(config: &AppConfig) {
    let path = config_path();
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = fs::write(path, json);
    }
}
