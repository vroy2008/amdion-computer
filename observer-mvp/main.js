const { app, BrowserWindow, desktopCapturer, ipcMain } = require('electron');
const path = require('path');
const fs = require('fs');

let nudgeWindow;

function createNudgeWindow() {
  nudgeWindow = new BrowserWindow({
    width: 600,
    height: 600,
    transparent: true,
    frame: false,
    alwaysOnTop: true,
    hasShadow: false,
    focusable: true,
    resizable: false,
    webPreferences: {
      nodeIntegration: true,
      contextIsolation: false
    }
  });

  // Position at the top center of the screen
  nudgeWindow.setPosition(Math.floor((require('electron').screen.getPrimaryDisplay().workAreaSize.width - 600) / 2), 50);

  nudgeWindow.loadFile('index.html');
  // Initially hidden, or we can just send it a signal to display text when ready
}

app.whenReady().then(() => {
  createNudgeWindow();

  app.on('activate', function () {
    if (BrowserWindow.getAllWindows().length === 0) createNudgeWindow();
  });
});

app.on('window-all-closed', function () {
  if (process.platform !== 'darwin') app.quit();
});

// Capture Loop Implementation
let captureInterval = null;
let isScanning = false;

async function performScan(isManual = false) {
  if (isScanning) return;
  isScanning = true;
  
  if (nudgeWindow) nudgeWindow.webContents.send('set-scanning-state', true);
  console.log(`Performing screen capture scan... (Manual: ${isManual})`);
  
  try {
    const sources = await desktopCapturer.getSources({ 
      types: ['screen'], 
      thumbnailSize: { width: 1280, height: 720 } 
    });

    if (sources.length > 0) {
      const primaryScreen = sources[0];
      const thumbnail = primaryScreen.thumbnail;
      const dataUrl = thumbnail.toDataURL(); 
      
      console.log(`Captured screen. Size: ${dataUrl.length} bytes`);
      
      // Pass this to the Gemini Logic Engine
      const analyzeScreenshot = require('./gemini').analyzeScreenshot;
      const analysis = await analyzeScreenshot(dataUrl, isManual);
      
      console.log("Gemini Analysis:", analysis);
      
      if (analysis && analysis.nudge) {
        // Show the nudge
        if (nudgeWindow) {
           nudgeWindow.webContents.send('show-nudge', {
             observation: analysis.observation,
             message: analysis.message,
             actionText: analysis.action
          });
        }
      }

    }
  } catch (error) {
    console.error("Capture error:", error);
  } finally {
    isScanning = false;
    if (nudgeWindow) nudgeWindow.webContents.send('set-scanning-state', false);
  }
}

function startCaptureLoop() {
  console.log("Capture loop started.");
  if (!captureInterval) {
    performScan(); // Execute immediately
    captureInterval = setInterval(performScan, 10000); // Check every 10 seconds for MVP
  }
}

function stopCaptureLoop() {
  console.log("Capture loop stopped.");
  if (captureInterval) {
    clearInterval(captureInterval);
    captureInterval = null;
  }
}

// Ensure IPC events are handled
ipcMain.on('set-scan-loop', (event, isContinuous) => {
    if (isContinuous) {
        startCaptureLoop();
    } else {
        stopCaptureLoop();
    }
});

ipcMain.on('trigger-manual-scan', () => {
    console.log("Manual scan triggered via Review & Assist.");
    performScan(true); // pass isManual=true
});

ipcMain.on('nudge-action-clicked', () => {
    console.log("User clicked the nudge action! Execute the logic here.");
    // Example: send keystrokes, run python script, etc.
});

ipcMain.on('send-chat-message', async (event, message) => {
    console.log("Chat message received:", message);
    if (nudgeWindow) nudgeWindow.webContents.send('set-scanning-state', true);
    
    try {
        const sources = await desktopCapturer.getSources({ 
            types: ['screen'], 
            thumbnailSize: { width: 1280, height: 720 } 
        });

        if (sources.length > 0) {
            const dataUrl = sources[0].thumbnail.toDataURL();
            const { chatWithScreenshot } = require('./gemini');
            const response = await chatWithScreenshot(dataUrl, message);
            if (nudgeWindow) nudgeWindow.webContents.send('chat-response', response);
        } else {
             if (nudgeWindow) nudgeWindow.webContents.send('chat-response', "Could not capture screen.");
        }
    } catch (error) {
        console.error("Chat error:", error);
        if (nudgeWindow) nudgeWindow.webContents.send('chat-response', "An error occurred during chat processing.");
    } finally {
        if (nudgeWindow) nudgeWindow.webContents.send('set-scanning-state', false);
    }
});

ipcMain.on('get-config', (event) => {
    const fs = require('fs');
    const configPath = path.join(__dirname, 'config.json');
    let config = { apiKey: process.env.GEMINI_API_KEY || '', model: 'gemini-3.1-flash-lite-preview' };
    if (fs.existsSync(configPath)) {
        try {
            const saved = JSON.parse(fs.readFileSync(configPath, 'utf8'));
            if (saved.apiKey) config.apiKey = saved.apiKey;
            if (saved.model) config.model = saved.model;
        } catch(e) {}
    }
    event.reply('config-data', config);
});

ipcMain.on('save-config', (event, config) => {
    const fs = require('fs');
    console.log("Saving new configuration...");
    const configPath = path.join(__dirname, 'config.json');
    fs.writeFileSync(configPath, JSON.stringify(config, null, 2));
    
    // In dev mode, app.relaunch doesn't run `npm start` again, it just exits.
    // So we just reply that it was saved, and let the UI tell the user to restart if needed, 
    // or we can just apply it immediately if we want.
    event.reply('config-saved');
});


