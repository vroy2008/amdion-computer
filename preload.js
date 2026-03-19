const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('electronAPI', {
  openApp: (appData) => ipcRenderer.invoke('open-app', appData),
  switchTab: (tabId) => ipcRenderer.invoke('switch-tab', tabId),
  closeTab: (tabId) => ipcRenderer.invoke('close-tab', tabId),
  toggleSidebar: () => ipcRenderer.invoke('toggle-sidebar'),
  toggleRightSidebar: () => ipcRenderer.invoke('toggle-right-sidebar'),
  goHome: () => ipcRenderer.invoke('go-home'),
  getState: () => ipcRenderer.invoke('get-state'),
  onStateUpdate: (callback) => ipcRenderer.on('state-update', (event, state) => callback(state)),

  // --- API Settings, Controls, & Data ---
  getConfig: () => ipcRenderer.send('get-config'),
  onConfigData: (callback) => ipcRenderer.on('config-data', (event, config) => callback(config)),
  saveConfig: (config) => ipcRenderer.send('save-config', config),
  onConfigSaved: (callback) => ipcRenderer.on('config-saved', () => callback()),
  getFavorites: () => ipcRenderer.invoke('get-favorites'),
  addFavorite: (appData) => ipcRenderer.invoke('add-favorite', appData),
  setLoopState: (state) => ipcRenderer.send('set-loop-state', state),
  triggerManualScan: () => ipcRenderer.send('trigger-manual-scan'),
  onSetScanningState: (callback) => ipcRenderer.on('set-scanning-state', (event, isScanning) => callback(isScanning)),

  // --- Chat & Responses ---
  sendChatMessage: (message) => ipcRenderer.send('send-chat-message', message),
  onChatResponse: (callback) => ipcRenderer.on('chat-response', (event, reply) => callback(reply)),
  onShowNudge: (callback) => ipcRenderer.on('show-nudge', (event, analysis) => callback(analysis)),

  // --- AI Agent ---
  sendAgentAction: (task) => ipcRenderer.send('send-agent-action', task),
  stopAgent: () => ipcRenderer.send('stop-agent'),
  onAgentUpdate: (callback) => ipcRenderer.on('agent-update', (event, update) => callback(update)),

  // --- Journal ---
  setJournalState: (state) => ipcRenderer.send('set-journal-state', state),
  getJournal: () => ipcRenderer.invoke('get-journal'),
  onJournalUpdate: (callback) => ipcRenderer.on('journal-update', (event, entry) => callback(entry)),
  getJournalDates: () => ipcRenderer.invoke('get-journal-dates'),
  getJournalByDate: (date) => ipcRenderer.invoke('get-journal-by-date', date),
  getJournalGraph: (date) => ipcRenderer.invoke('get-journal-graph', date),
  transcribeAudio: (base64) => ipcRenderer.invoke('transcribe-audio', base64)
});
