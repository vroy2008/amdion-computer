// bridge.js — exposes the Tauri backend to the front door as `window.amdion`.
// Loaded only under Tauri (the only runtime now that Electron is gone).

if (window.__TAURI__) {
  const { invoke } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;

  window.amdion = {
    // ── Front-door state ──
    getState: () => invoke('get_state'),
    onStateUpdate: (cb) => listen('state-update', (e) => cb(e.payload)),
    setIntent: (intent) => invoke('set_intent', { intent }),
    hidePanel: () => invoke('hide_panel'),
    expandForOnboarding: () => invoke('expand_for_onboarding'),
    retreatToMenubar: () => invoke('retreat_to_menubar'),

    // ── Config ──
    getConfig: () => invoke('get_config'),
    saveConfig: (config) => invoke('save_config', { config }),
    setSummonShortcut: (accelerator) => invoke('set_summon_shortcut', { accelerator }),

    // ── Read Mode ──
    enterReadMode: () => invoke('enter_read_mode'),
    exitReadMode: () => invoke('exit_read_mode'),

    // ── Observer (Step 3): daily stats over the local event store ──
    getDailySummary: (date) => invoke('get_daily_summary', { date: date || null }),
    getSessions: (date) => invoke('get_sessions', { date: date || null }),
    onSensingUpdate: (cb) => listen('sensing-update', (e) => cb(e.payload)),

    // ── Mac tuning ──
    listMacTweaks: () => invoke('list_mac_tweaks'),
    applyMacTuning: (keys) => invoke('apply_mac_tuning', { keys }),
    revertMacTuning: (keys) => invoke('revert_mac_tuning', { keys }),
    openSettingsPane: (pane) => invoke('open_settings_pane', { pane }),

    // ── Updates ──
    checkForUpdates: () => invoke('check_for_updates'),
    relaunchApp: () => invoke('relaunch_app'),
  };
}
