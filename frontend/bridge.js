// bridge.js — Maps window.electronAPI to Tauri invoke/listen
// This file is loaded ONLY when running under Tauri.
// When running under Electron, the preload.js sets window.electronAPI instead.

if (window.__TAURI__) {
  const { invoke } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;

  // Internal callback stores for send+reply patterns
  const _callbacks = {};

  window.electronAPI = {
    // ── Window / Tab Management ──
    openApp: (appData) => invoke('open_app', { appData }),
    switchTab: (tabId) => invoke('switch_tab', { tabId }),
    closeTab: (tabId) => invoke('close_tab', { tabId }),
    goHome: () => invoke('go_home'),
    toggleSidebar: () => invoke('toggle_sidebar'),
    toggleRightSidebar: () => invoke('toggle_right_sidebar'),
    getState: () => invoke('get_state'),

    onStateUpdate: (callback) => {
      listen('state-update', (event) => callback(event.payload));
    },

    // ── Config ──
    getConfig: () => {
      invoke('get_config').then((config) => {
        if (_callbacks.onConfigData) _callbacks.onConfigData(config);
      });
    },
    onConfigData: (callback) => { _callbacks.onConfigData = callback; },

    saveConfig: (config) => {
      invoke('save_config', { config }).then(() => {
        if (_callbacks.onConfigSaved) _callbacks.onConfigSaved();
      });
    },
    onConfigSaved: (callback) => { _callbacks.onConfigSaved = callback; },

    getFavorites: () => invoke('get_favorites'),
    addFavorite: (appData) => invoke('add_favorite', { appData }),

    // ── AI Scanning ──
    setLoopState: (state) => invoke('set_loop_state', { stateVal: state }),
    triggerManualScan: () => invoke('trigger_manual_scan'),

    onSetScanningState: (callback) => {
      listen('set-scanning-state', (event) => callback(event.payload));
    },

    // ── Chat ──
    sendChatMessage: (message) => invoke('send_chat_message', { message }),

    onChatResponse: (callback) => {
      listen('chat-response', (event) => callback(event.payload));
    },

    onShowNudge: (callback) => {
      listen('show-nudge', (event) => callback(event.payload));
    },

    // ── Agent ──
    sendAgentAction: (task) => invoke('send_agent_action', { task }),
    stopAgent: () => invoke('stop_agent'),

    onAgentUpdate: (callback) => {
      listen('agent-update', (event) => callback(event.payload));
    },

    // ── Journal ──
    setJournalState: (state) => invoke('set_journal_state', { stateVal: state }),
    getJournal: () => invoke('get_journal'),

    onJournalUpdate: (callback) => {
      listen('journal-update', (event) => callback(event.payload));
    },

    getJournalDates: () => invoke('get_journal_dates'),
    getJournalByDate: (date) => invoke('get_journal_by_date', { date }),
    getJournalGraph: (date) => invoke('get_journal_graph', { date }),
    transcribeAudio: (base64) => invoke('transcribe_audio', { base64Audio: base64 }),
  };
}
