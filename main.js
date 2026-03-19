const { app, BrowserWindow, BrowserView, ipcMain, globalShortcut, nativeImage, desktopCapturer } = require('electron');
const path = require('path');
const fs = require('fs');
const { analyzeScreenshot, chatWithScreenshot, agentAction, summarizeActivity, extractTopics, transcribeAudio } = require('./gemini.js');

// Prevent EPIPE crashes when stdout pipe breaks
process.on('uncaughtException', (err) => {
    if (err.code === 'EPIPE') return;
    console.error('Uncaught exception:', err);
});

const MAX_AGENT_STEPS = 10;
let agentRunning = false;

// Journal state
const JOURNAL_INTERVAL_MS = 120000; // 2 minutes
let journalRecording = false;
let journalInterval = null;

let mainWindow;
let contentView;
let activeTabs = [];
let activeTabId = null;
let isHome = true;

const SIDEBAR_WIDTH = 250;
const SIDEBAR_COLLAPSED_WIDTH = 60;
let sidebarCollapsed = false;

const RIGHT_SIDEBAR_WIDTH = 300;
let rightSidebarHidden = false;

let isScanning = false;
let scanInterval;
const SCAN_INTERVAL_MS = 60000;

function createWindow() {
  // Use .ico for Windows, .png for others
  const iconExt = process.platform === 'win32' ? 'icon.ico' : 'icon.png';
  const icon = nativeImage.createFromPath(path.join(__dirname, iconExt));
  
  mainWindow = new BrowserWindow({
    width: 1400,
    height: 900,
    minWidth: 800,
    minHeight: 600,
    titleBarStyle: 'hidden',
    trafficLightPosition: { x: 20, y: 20 },
    fullscreenable: true,
    simpleFullscreen: true,
    backgroundColor: '#0a0a0a',
    icon: icon,
    webPreferences: {
      nodeIntegration: false,
      contextIsolation: true,
      preload: path.join(__dirname, 'preload.js')
    }
  });
  
  // Set dock icon on macOS
  if (process.platform === 'darwin') {
    app.dock.setIcon(icon);
  }

  // Enter fullscreen mode
  mainWindow.setFullScreen(true);

  // Load the main UI
  mainWindow.loadFile('index.html');

  // Grant microphone permission for speech recognition
  const { session } = mainWindow.webContents;
  session.setPermissionRequestHandler((webContents, permission, callback) => {
    const allowed = ['media', 'microphone', 'audioCapture'].includes(permission);
    callback(allowed);
  });

  // Handle window resize
  mainWindow.on('resize', () => {
    updateViewBounds();
  });

  // Register global shortcut to show/hide
  globalShortcut.register('CommandOrControl+Shift+Space', () => {
    if (mainWindow.isVisible()) {
      mainWindow.hide();
    } else {
      mainWindow.show();
      mainWindow.focus();
    }
  });
}

function updateViewBounds() {
  if (!mainWindow || !contentView) return;
  
  const [width, height] = mainWindow.getContentSize();
  const leftSidebarWidth = sidebarCollapsed ? SIDEBAR_COLLAPSED_WIDTH : SIDEBAR_WIDTH;
  const rightSidebarWidth = rightSidebarHidden ? 0 : RIGHT_SIDEBAR_WIDTH;
  
  contentView.setBounds({
    x: leftSidebarWidth,
    y: 0,
    width: width - leftSidebarWidth - rightSidebarWidth,
    height: height
  });
}

function openApp(appData) {
  if (!mainWindow) return;
  
  const { id, name, url } = appData;
  
  // Add to active tabs if not already there
  if (!activeTabs.find(t => t.id === id)) {
    activeTabs.push({ id, name, url });
  }
  activeTabId = id;
  isHome = false;
  
  // Remove existing content view if any
  if (contentView) {
    mainWindow.removeBrowserView(contentView);
  }
  
  // Create new BrowserView for the app
  contentView = new BrowserView({
    webPreferences: {
      nodeIntegration: false,
      contextIsolation: true
    }
  });
  contentView.webContents.setUserAgent('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36');
  
  mainWindow.addBrowserView(contentView);
  updateViewBounds();
  
  // Load the URL
  contentView.webContents.loadURL(url);
  
  // Notify renderer to update UI
  mainWindow.webContents.send('state-update', { activeTabs, activeTabId, isHome, sidebarCollapsed, rightSidebarHidden });
}

function switchToTab(tabId) {
  const tab = activeTabs.find(t => t.id === tabId);
  if (tab) {
    activeTabId = tabId;
    isHome = false;
    
    if (contentView) {
      mainWindow.removeBrowserView(contentView);
    }
    
    contentView = new BrowserView({
      webPreferences: {
        nodeIntegration: false,
        contextIsolation: true
      }
    });
    contentView.webContents.setUserAgent('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36');
    
    mainWindow.addBrowserView(contentView);
    updateViewBounds();
    contentView.webContents.loadURL(tab.url);
    
    mainWindow.webContents.send('state-update', { activeTabs, activeTabId, isHome, sidebarCollapsed, rightSidebarHidden });
  }
}

function closeTab(tabId) {
  activeTabs = activeTabs.filter(t => t.id !== tabId);
  
  if (activeTabId === tabId) {
    if (activeTabs.length > 0) {
      switchToTab(activeTabs[activeTabs.length - 1].id);
    } else {
      goHome();
    }
  }
  
  mainWindow.webContents.send('state-update', { activeTabs, activeTabId, isHome, sidebarCollapsed, rightSidebarHidden });
}

function goHome() {
  if (contentView) {
    mainWindow.removeBrowserView(contentView);
    contentView = null;
  }
  activeTabId = null;
  isHome = true;
  
  mainWindow.webContents.send('state-update', { activeTabs, activeTabId, isHome, sidebarCollapsed, rightSidebarHidden });
}

function toggleSidebar() {
  sidebarCollapsed = !sidebarCollapsed;
  updateViewBounds();
  return sidebarCollapsed;
}

function toggleRightSidebar() {
  rightSidebarHidden = !rightSidebarHidden;
  
  if (rightSidebarHidden) {
    // Hiding: expand view first, then update UI
    updateViewBounds();
    if (mainWindow) {
        mainWindow.webContents.send('state-update', { activeTabs, activeTabId, isHome, sidebarCollapsed, rightSidebarHidden });
    }
  } else {
    // Showing: update UI first to paint DOM, then shrink view
    if (mainWindow) {
        mainWindow.webContents.send('state-update', { activeTabs, activeTabId, isHome, sidebarCollapsed, rightSidebarHidden });
    }
    setTimeout(() => {
      updateViewBounds();
    }, 50);
  }
  return rightSidebarHidden;
}

// AI Scanning Service Functions
async function captureScreen() {
    try {
        const sources = await desktopCapturer.getSources({ types: ['screen'], thumbnailSize: { width: 1280, height: 720 } });
        if (sources.length > 0) {
            const thumbnail = sources[0].thumbnail;
            const dataUrl = thumbnail.toDataURL();
            return dataUrl;
        }
        return null;
    } catch (e) {
        console.error("Error capturing screen:", e);
        return null;
    }
}

async function performScan(isManual = false) {
    if (!mainWindow) return;
    try {
        console.log(`Performing screen capture scan... (Manual: ${isManual})`);
        
        // Notify UI scanning started
        mainWindow.webContents.send('set-scanning-state', true);

        const dataUrl = await captureScreen();
        if (!dataUrl) {
            console.log("Failed to capture screen.");
            return;
        }

        const analysis = await analyzeScreenshot(dataUrl, isManual);
        console.log("Gemini Analysis:", analysis);

        if (analysis && analysis.nudge) {
            // Forward AI nudge to the UI
            mainWindow.webContents.send('show-nudge', analysis);
        }

    } catch (error) {
        console.error("Scan error:", error);
    } finally {
        // Notify UI scanning finished
        if (mainWindow) {
            mainWindow.webContents.send('set-scanning-state', false);
        }
    }
}

// IPC handlers - Window Management & State
ipcMain.handle('open-app', (event, appData) => {
  openApp(appData);
  return { success: true, activeTabs, activeTabId, isHome };
});

ipcMain.handle('switch-tab', (event, tabId) => {
  switchToTab(tabId);
  return { success: true };
});

ipcMain.handle('close-tab', (event, tabId) => {
  closeTab(tabId);
  return { success: true };
});

ipcMain.handle('toggle-sidebar', () => {
  const collapsed = toggleSidebar();
  return { collapsed };
});

ipcMain.handle('toggle-right-sidebar', () => {
  const hidden = toggleRightSidebar();
  return { hidden };
});

ipcMain.handle('go-home', () => {
  goHome();
  return { success: true };
});

ipcMain.handle('get-state', () => {
  return { activeTabs, activeTabId, isHome, sidebarCollapsed, rightSidebarHidden };
});

// IPC handlers - AI Settings, Controls, & Data
ipcMain.on('get-config', (event) => {
    const configPath = path.join(__dirname, 'config.json');
    let config = { 
        apiKey: process.env.GEMINI_API_KEY || '', 
        model: 'gemini-3.1-flash-lite-preview',
        favorites: [
            { id: '1', name: 'Notion', url: 'https://notion.so' },
            { id: '2', name: 'Spotify', url: 'https://open.spotify.com' },
            { id: '3', name: 'Antigravity', url: 'https://antigravity.com' },
            { id: '4', name: 'Drive', url: 'https://drive.google.com' }
        ]
    };
    
    if (fs.existsSync(configPath)) {
        try {
            const saved = JSON.parse(fs.readFileSync(configPath, 'utf8'));
            if (saved.apiKey) config.apiKey = saved.apiKey;
            if (saved.model) config.model = saved.model;
            if (saved.favorites && Array.isArray(saved.favorites)) config.favorites = saved.favorites;
        } catch(e) {}
    }
    event.reply('config-data', config);
});

ipcMain.on('save-config', (event, configUpdate) => {
    const configPath = path.join(__dirname, 'config.json');
    let currentConfig = { favorites: [] };
    
    // Read existing config so we don't wipe out favorites when saving AI settings
    if (fs.existsSync(configPath)) {
        try {
            const saved = JSON.parse(fs.readFileSync(configPath, 'utf8'));
            currentConfig = { ...saved };
        } catch(e) {}
    }

    // Merge updates
    const newConfig = { ...currentConfig, ...configUpdate };
    
    fs.writeFileSync(configPath, JSON.stringify(newConfig, null, 2));
    event.reply('config-saved');
});

ipcMain.handle('get-favorites', (event) => {
    const configPath = path.join(__dirname, 'config.json');
    let favorites = [
        { id: '1', name: 'Notion', url: 'https://notion.so' },
        { id: '2', name: 'Spotify', url: 'https://open.spotify.com' },
        { id: '3', name: 'Drive', url: 'https://drive.google.com' },
        { id: '4', name: 'Docs', url: 'https://docs.google.com' },
        { id: '5', name: 'Antigravity', url: 'https://mrdoob.com/projects/chromeexperiments/google-gravity/' }
    ];

    if (fs.existsSync(configPath)) {
        try {
            const saved = JSON.parse(fs.readFileSync(configPath, 'utf8'));
            if (saved.favorites && Array.isArray(saved.favorites)) {
                favorites = saved.favorites;
            }
        } catch(e) {}
    }
    return favorites;
});

ipcMain.handle('add-favorite', (event, appData) => {
    const configPath = path.join(__dirname, 'config.json');
    let currentConfig = { 
        apiKey: '', 
        model: 'gemini-3.1-flash-lite-preview',
        favorites: [
            { id: '1', name: 'Notion', url: 'https://notion.so' },
            { id: '2', name: 'Spotify', url: 'https://open.spotify.com' },
            { id: '3', name: 'Antigravity', url: 'https://antigravity.com' },
            { id: '4', name: 'Drive', url: 'https://drive.google.com' }
        ]
    };
    
    if (fs.existsSync(configPath)) {
        try {
            const saved = JSON.parse(fs.readFileSync(configPath, 'utf8'));
            currentConfig = { ...currentConfig, ...saved };
        } catch(e) {}
    }

    // Generate unique ID
    const newId = Date.now().toString();
    const newFavorite = { id: newId, ...appData };
    
    currentConfig.favorites.push(newFavorite);
    fs.writeFileSync(configPath, JSON.stringify(currentConfig, null, 2));
    
    return currentConfig.favorites;
});

ipcMain.on('set-loop-state', (event, state) => {
    isScanning = state;
    if (isScanning) {
        performScan(false); // Do an initial background scan immediately
        scanInterval = setInterval(() => performScan(false), SCAN_INTERVAL_MS);
    } else {
        clearInterval(scanInterval);
    }
});

// Manual Nudge Request
ipcMain.on('trigger-manual-scan', () => {
    performScan(true);
});

// Direct Chat Requests
ipcMain.on('send-chat-message', async (event, message) => {
    try {
        console.log("Chat message received inside AI workspace:", message);
        
        // Log to journal if recording
        if (journalRecording) {
            appendJournalEntry('chat_user', { message });
        }

        const dataUrl = await captureScreen();
        if (!dataUrl) {
            event.reply('chat-response', "I couldn't view your screen to answer that.");
            return;
        }

        const reply = await chatWithScreenshot(dataUrl, message);
        event.reply('chat-response', reply);
        
        // Log AI reply to journal
        if (journalRecording) {
            appendJournalEntry('chat_ai', { message: reply });
        }
    } catch (error) {
        console.error("Chat processing error:", error);
        event.reply('chat-response', "Sorry, I ran into an error processing your message.");
    }
});

// =============================================
// AGENT IPC handlers
// =============================================
ipcMain.on('send-agent-action', async (event, task) => {
    if (!contentView) {
        event.reply('agent-update', { type: 'error', message: 'Please open an app first before asking the agent to interact with it.' });
        event.reply('agent-update', { type: 'finished' });
        return;
    }
    if (journalRecording) {
        appendJournalEntry('agent_task', { task });
    }
    runAgentLoop(task);
});

ipcMain.on('stop-agent', () => {
    agentRunning = false;
});

// =============================================
// DAILY JOURNAL ENGINE
// =============================================

function getJournalPath() {
    const today = new Date().toISOString().split('T')[0]; // YYYY-MM-DD
    const journalsDir = path.join(__dirname, 'journals');
    if (!fs.existsSync(journalsDir)) {
        fs.mkdirSync(journalsDir, { recursive: true });
    }
    return path.join(journalsDir, `${today}.json`);
}

function appendJournalEntry(type, data) {
    try {
        const journalPath = getJournalPath();
        let entries = [];
        if (fs.existsSync(journalPath)) {
            entries = JSON.parse(fs.readFileSync(journalPath, 'utf-8'));
        }
        
        const entry = {
            timestamp: new Date().toISOString(),
            time: new Date().toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' }),
            type,
            ...data
        };
        
        entries.push(entry);
        fs.writeFileSync(journalPath, JSON.stringify(entries, null, 2));
        
        // Notify frontend of new entry
        if (mainWindow) {
            mainWindow.webContents.send('journal-update', entry);
        }
        
        return entry;
    } catch(e) {
        console.error('Journal write error:', e);
    }
}

async function performJournalCapture() {
    if (!mainWindow || !journalRecording) return;
    try {
        const dataUrl = await captureScreen();
        if (!dataUrl) return;
        
        const summary = await summarizeActivity(dataUrl);
        if (summary) {
            appendJournalEntry('activity', { summary });
        }
    } catch(e) {
        console.error('Journal capture error:', e);
    }
}

// Journal IPC handlers
ipcMain.on('set-journal-state', (event, state) => {
    journalRecording = state;
    if (journalRecording) {
        appendJournalEntry('journal_start', { summary: 'Recording started' });
        performJournalCapture(); // Initial capture
        journalInterval = setInterval(() => performJournalCapture(), JOURNAL_INTERVAL_MS);
    } else {
        appendJournalEntry('journal_stop', { summary: 'Recording stopped' });
        clearInterval(journalInterval);
        journalInterval = null;
    }
});

ipcMain.handle('get-journal', () => {
    try {
        const journalPath = getJournalPath();
        if (fs.existsSync(journalPath)) {
            return JSON.parse(fs.readFileSync(journalPath, 'utf-8'));
        }
        return [];
    } catch(e) {
        return [];
    }
});

ipcMain.handle('get-journal-dates', () => {
    try {
        const journalsDir = path.join(__dirname, 'journals');
        if (!fs.existsSync(journalsDir)) return [];
        return fs.readdirSync(journalsDir)
            .filter(f => f.endsWith('.json'))
            .map(f => f.replace('.json', ''))
            .sort()
            .reverse();
    } catch(e) {
        return [];
    }
});

ipcMain.handle('get-journal-by-date', (event, date) => {
    try {
        const filePath = path.join(__dirname, 'journals', `${date}.json`);
        if (fs.existsSync(filePath)) {
            return JSON.parse(fs.readFileSync(filePath, 'utf-8'));
        }
        return [];
    } catch(e) {
        return [];
    }
});

ipcMain.handle('get-journal-graph', async (event, date) => {
    try {
        const filePath = path.join(__dirname, 'journals', `${date}.json`);
        if (!fs.existsSync(filePath)) return { nodes: [], edges: [] };
        const entries = JSON.parse(fs.readFileSync(filePath, 'utf-8'));
        if (entries.length === 0) return { nodes: [], edges: [] };
        return await extractTopics(entries);
    } catch(e) {
        console.error('Graph extraction error:', e);
        return { nodes: [], edges: [] };
    }
});

ipcMain.handle('transcribe-audio', async (event, base64Audio) => {
    try {
        return await transcribeAudio(base64Audio);
    } catch(e) {
        console.error('Transcription error:', e);
        return null;
    }
});

async function captureActiveView() {
    try {
        if (contentView && contentView.webContents) {
            const image = await contentView.webContents.capturePage();
            const resized = image.resize({ width: 1280, height: 720 });
            return resized.toDataURL();
        }
        // Fallback to desktop capture
        return await captureScreen();
    } catch(e) {
        console.error("Error capturing active view:", e);
        return await captureScreen();
    }
}

async function executeAgentAction(action) {
    if (!contentView || !contentView.webContents) return;

    const wc = contentView.webContents;

    switch(action.type) {
        case 'click': {
            // Scale coordinates from 1280x720 screenshot to actual view size
            const bounds = contentView.getBounds();
            const scaleX = bounds.width / 1280;
            const scaleY = bounds.height / 720;
            const x = Math.round(action.x * scaleX);
            const y = Math.round(action.y * scaleY);
            
            // PRIMARY: Use Chromium-level native click (sendInputEvent)
            // This works for most web apps including complex SPAs
            wc.sendInputEvent({ type: 'mouseDown', x, y, button: 'left', clickCount: 1 });
            await new Promise(r => setTimeout(r, 80));
            wc.sendInputEvent({ type: 'mouseUp', x, y, button: 'left', clickCount: 1 });
            
            await new Promise(r => setTimeout(r, 600));
            break;
        }
        case 'type': {
            // Strategy 1: DOM-level for standard <input>/<textarea>
            let usedDom = false;
            try {
                usedDom = await wc.executeJavaScript(`
                    (function() {
                        const el = document.activeElement;
                        if (el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA')) {
                            el.value = (el.value || '') + ${JSON.stringify(action.text)};
                            el.dispatchEvent(new Event('input', { bubbles: true }));
                            el.dispatchEvent(new Event('change', { bubbles: true }));
                            return true;
                        }
                        return false;
                    })()
                `);
            } catch(e) {
                usedDom = false;
            }

            if (!usedDom) {
                // Strategy 2: insertText at OS IME level
                try {
                    await wc.insertText(action.text);
                } catch(e1) {
                    // Strategy 3: character-by-character sendInputEvent
                    for (const char of action.text) {
                        wc.sendInputEvent({ type: 'keyDown', keyCode: char });
                        wc.sendInputEvent({ type: 'char', keyCode: char });
                        wc.sendInputEvent({ type: 'keyUp', keyCode: char });
                        await new Promise(r => setTimeout(r, 50));
                    }
                }
            }
            await new Promise(r => setTimeout(r, 500));
            break;
        }
        case 'press': {
            const keyMap = {
                'Enter': '\r', 'Tab': '\t', 'Escape': '\u001b',
                'Backspace': '\b', 'Space': ' ',
                'ArrowUp': 'Up', 'ArrowDown': 'Down',
                'ArrowLeft': 'Left', 'ArrowRight': 'Right'
            };
            const key = keyMap[action.key] || action.key;
            wc.sendInputEvent({ type: 'keyDown', keyCode: key });
            wc.sendInputEvent({ type: 'keyUp', keyCode: key });
            await new Promise(r => setTimeout(r, 300));
            break;
        }
        case 'scroll': {
            const amount = action.amount || 300;
            const dir = action.direction === 'down' ? 1 : -1;
            try {
                await wc.executeJavaScript(`window.scrollBy(0, ${amount * dir})`);
            } catch(e) {
                const deltaY = amount * dir;
                wc.sendInputEvent({ type: 'mouseWheel', x: 640, y: 360, deltaX: 0, deltaY });
            }
            await new Promise(r => setTimeout(r, 500));
            break;
        }
        case 'wait': {
            await new Promise(r => setTimeout(r, action.ms || 1000));
            break;
        }
    }
}

async function runAgentLoop(task) {
    if (agentRunning) {
        mainWindow.webContents.send('agent-update', { type: 'error', message: 'Agent is already running a task.' });
        return;
    }
    
    agentRunning = true;
    const history = [];
    
    mainWindow.webContents.send('agent-update', { type: 'start', message: `Starting task: "${task}"` });
    
    try {
        for (let step = 0; step < MAX_AGENT_STEPS; step++) {
            // Check if cancelled
            if (!agentRunning) {
                mainWindow.webContents.send('agent-update', { type: 'done', message: 'Agent stopped by user.' });
                break;
            }

            // 1. Capture what the agent sees
            mainWindow.webContents.send('agent-update', { type: 'thinking', message: `Step ${step + 1}: Observing screen...` });
            
            await new Promise(r => setTimeout(r, 500)); // Let any animations settle
            const dataUrl = await captureActiveView();
            
            if (!dataUrl) {
                mainWindow.webContents.send('agent-update', { type: 'error', message: "Couldn't capture the screen." });
                break;
            }
            
            // Check if cancelled
            if (!agentRunning) {
                mainWindow.webContents.send('agent-update', { type: 'done', message: 'Agent stopped by user.' });
                break;
            }

            // 2. Ask Gemini what to do
            const response = await agentAction(dataUrl, task, history);
            try { console.log(`Agent step ${step + 1}:`, JSON.stringify(response, null, 2)); } catch(e) {}
            
            // 3. Show the agent's thinking
            if (response.thinking) {
                mainWindow.webContents.send('agent-update', { type: 'thinking', message: response.thinking });
            }

            // 4. Check if done
            if (response.done) {
                mainWindow.webContents.send('agent-update', { 
                    type: 'done', 
                    message: response.message || 'Task completed!' 
                });
                break;
            }

            // 5. Execute actions
            if (response.actions && response.actions.length > 0) {
                for (const action of response.actions) {
                    if (!agentRunning) break;
                    const desc = action.description || `${action.type}`;
                    mainWindow.webContents.send('agent-update', { type: 'action', message: `🖱️ ${desc}` });
                    history.push(desc);
                    
                    await executeAgentAction(action);
                }
            }
            
            // 6. Brief pause before next observation
            await new Promise(r => setTimeout(r, 800));
        }
    } catch (error) {
        console.error("Agent loop error:", error);
        mainWindow.webContents.send('agent-update', { type: 'error', message: 'Agent encountered an error: ' + error.message });
    } finally {
        agentRunning = false;
        mainWindow.webContents.send('agent-update', { type: 'finished' });
    }
}

app.whenReady().then(() => {
  createWindow();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

app.on('will-quit', () => {
  globalShortcut.unregisterAll();
});
