const { GoogleGenAI } = require('@google/genai');
const fs = require('fs');
const path = require('path');
require('dotenv').config();

function getConfig() {
  const configPath = path.join(__dirname, 'config.json');
  let config = { apiKey: process.env.GEMINI_API_KEY, model: 'gemini-3.1-flash-lite-preview' };
  if (fs.existsSync(configPath)) {
    try {
      const saved = JSON.parse(fs.readFileSync(configPath, 'utf8'));
      if (saved.apiKey) config.apiKey = saved.apiKey;
      if (saved.model) config.model = saved.model;
    } catch(e) {}
  }
  return config;
}

const SYSTEM_PROMPT_BACKGROUND = `
You are Amdion, a minimalist focus assistant.
The user is currently looking at this screen layout or actively working on a task.
Analyze the user's screen implicitly to understand their context.
Identify if they are performing a repetitive task, struggling with an error, or transitioning context.

If—and ONLY IF—you can offer a high-value, 1-click optimization or action that saves them time, output a JSON response in the following format:
{
  "nudge": true,
  "observation": "A 1-to-2 sentence observation of what the user is currently doing.",
  "message": "A question asking for the next step, or a high-value suggested next step.",
  "action": "A 1-to-2 word action button label (e.g., 'Generate', 'Fix')"
}

If no immediate, high-value action is necessary, or if the user is deep in focus on a normal task, you MUST stay invisible.
Do NOT be intrusive. Do not state what the user is doing. Only offer a nudge if it's highly actionable.
If you decide to stay invisible, output perfectly valid JSON:
{
  "nudge": false
}

Respond ONLY with valid JSON. No markdown formatting (\`\`\`json). Just the raw JSON object.
`;

const SYSTEM_PROMPT_MANUAL = `
You are Amdion, a minimalist focus assistant.
The user has clicked "Review and Assist" to manually ask for your help based on their current screen.
Analyze the user's screen to understand their context.

Since this is a manual request, you MUST provide an observation of what they are doing, and suggest a next step or ask a helpful question about how to proceed.
Output a JSON response in the following format:
{
  "nudge": true,
  "observation": "Briefly state what the user is working on.",
  "message": "Suggest a next step, automation, or ask a helpful question.",
  "action": "Action label (e.g. 'Execute', 'Summarize', 'Continue')"
}

Respond ONLY with valid JSON. No markdown formatting (\`\`\`json). Just the raw JSON object.
`;

async function analyzeScreenshot(dataUrl, isManual = false) {
  try {
    const config = getConfig();
    if (!config.apiKey) {
      console.warn("No GEMINI_API_KEY found. Returning nudge: false.");
      return { nudge: false };
    }

    const ai = new GoogleGenAI({ apiKey: config.apiKey });

    const mimeTypeStr = dataUrl.split(';')[0];
    const mimeType = mimeTypeStr.split(':')[1];
    const base64Data = dataUrl.split(',')[1];
    const promptToUse = isManual ? SYSTEM_PROMPT_MANUAL : SYSTEM_PROMPT_BACKGROUND;

    const modelResponse = await ai.models.generateContent({
      model: config.model,
      contents: [
        { role: 'user', parts: [
            { text: promptToUse },
            { 
               inlineData: {
                 mimeType: mimeType,
                 data: base64Data
               }
            }
          ]
        }
      ]
    });

    const textResponse = modelResponse.text;
    
    let cleanedResponse = textResponse.trim();
    if (cleanedResponse.startsWith('```json')) cleanedResponse = cleanedResponse.substring(7);
    if (cleanedResponse.startsWith('```')) cleanedResponse = cleanedResponse.substring(3);
    if (cleanedResponse.endsWith('```')) cleanedResponse = cleanedResponse.substring(0, cleanedResponse.length - 3);

    const parsedJson = JSON.parse(cleanedResponse.trim());
    return parsedJson;

  } catch (error) {
    console.error("Error analyzing screenshot with Gemini:", error);
    if (error.status === 429) {
       return { 
           nudge: true, 
           observation: "API Error", 
           message: "You have exceeded your Gemini API Free Tier Quota (15 requests/day).", 
           action: "Got it" 
       };
    }
    return { nudge: false };
  }
}

async function chatWithScreenshot(dataUrl, userMessage) {
  try {
    const config = getConfig();
    if (!config.apiKey) return "API Key is missing. Please configure it in Settings.";

    const ai = new GoogleGenAI({ apiKey: config.apiKey });

    const mimeTypeStr = dataUrl.split(';')[0];
    const mimeType = mimeTypeStr.split(':')[1];
    const base64Data = dataUrl.split(',')[1];

    const prompt = `You are Amdion, a minimalist focus assistant.
The user is asking you: "${userMessage}"
Use the provided screenshot of their current workspace to answer. Be concise, direct, and helpful.`;

    const modelResponse = await ai.models.generateContent({
      model: config.model,
      contents: [
        { role: 'user', parts: [
            { text: prompt },
            { 
               inlineData: {
                 mimeType: mimeType,
                 data: base64Data
               }
            }
          ]
        }
      ]
    });

    return modelResponse.text.trim();
  } catch (error) {
    console.error("Error asking Gemini via chat:", error);
    if (error.status === 429) return "Error: You have exceeded your Gemini API Free Tier Quota.";
    return "Sorry, I encountered an error answering that.";
  }
}

const SYSTEM_PROMPT_AGENT = `
You are Amdion Agent, an AI that can SEE and INTERACT with web pages inside a desktop application.
The user has given you a task to perform on the currently visible web page.
You receive a screenshot of the page. Analyze it carefully to determine what actions to take.

You MUST respond with valid JSON in this exact format:
{
  "thinking": "Brief explanation of what you see and what you plan to do next",
  "actions": [
    { "type": "click", "x": 400, "y": 300, "description": "Click the search bar" }
  ],
  "done": false,
  "message": "Optional message to show the user about what you're doing"
}

Available action types:
- { "type": "click", "x": <number>, "y": <number>, "description": "what you're clicking" }
- { "type": "type", "text": "<text to type>", "description": "what you're typing" }
- { "type": "press", "key": "<key name e.g. Enter, Tab>", "description": "what key" }
- { "type": "scroll", "direction": "up" or "down", "amount": <pixels>, "description": "why scrolling" }
- { "type": "wait", "ms": <milliseconds>, "description": "why waiting" }

IMPORTANT RULES:
1. The screenshot dimensions are 1280x720. All x,y coordinates must be relative to this resolution.
2. Return at most 3 actions per response. After they execute, you'll get a new screenshot to verify.
3. Set "done": true ONLY when the task is fully completed or if you cannot proceed.
4. If you cannot determine how to proceed, set "done": true and explain in "message".
5. Be precise with click coordinates — aim for the CENTER of buttons/links.
6. After typing, you usually need to press Enter or click a submit button.
7. Return ONLY valid JSON. No markdown, no code fences.
8. For document editors (Google Docs, Notion, etc): click the document BODY AREA first to focus it, then use "type" to insert text directly. Do NOT try to use the app's own AI features (like "Help me write"). Type the content yourself.
9. If a click doesn't work after 2 attempts at the same location, try a DIFFERENT approach (different element, different coordinates, or a different strategy entirely).
10. Keep your text output concise when typing — don't write excessively long paragraphs.
11. After typing text, do NOT set "done": true in the same step. Always wait for the next screenshot to VERIFY the text actually appeared before marking done.
`;

async function agentAction(dataUrl, userTask, history = []) {
  try {
    const config = getConfig();
    if (!config.apiKey) {
      return { thinking: "No API key", actions: [], done: true, message: "API Key is missing. Please configure it in Settings." };
    }

    const ai = new GoogleGenAI({ apiKey: config.apiKey });

    const mimeTypeStr = dataUrl.split(';')[0];
    const mimeType = mimeTypeStr.split(':')[1];
    const base64Data = dataUrl.split(',')[1];

    let historyContext = '';
    if (history.length > 0) {
      historyContext = `\n\nPrevious steps taken:\n${history.map((h, i) => `${i + 1}. ${h}`).join('\n')}\n\nThis is a follow-up screenshot AFTER those actions were executed. Analyze what happened and decide the next step.`;
    }

    const prompt = `${SYSTEM_PROMPT_AGENT}\n\nUser's task: "${userTask}"${historyContext}`;

    const modelResponse = await ai.models.generateContent({
      model: config.model,
      contents: [
        { role: 'user', parts: [
            { text: prompt },
            { 
               inlineData: {
                 mimeType: mimeType,
                 data: base64Data
               }
            }
          ]
        }
      ]
    });

    const textResponse = modelResponse.text;
    
    let cleanedResponse = textResponse.trim();
    if (cleanedResponse.startsWith('```json')) cleanedResponse = cleanedResponse.substring(7);
    if (cleanedResponse.startsWith('```')) cleanedResponse = cleanedResponse.substring(3);
    if (cleanedResponse.endsWith('```')) cleanedResponse = cleanedResponse.substring(0, cleanedResponse.length - 3);

    const parsed = JSON.parse(cleanedResponse.trim());
    return parsed;

  } catch (error) {
    console.error("Error in agent action:", error);
    if (error.status === 429) {
      return { thinking: "Rate limited", actions: [], done: true, message: "API rate limit reached. Try again later." };
    }
    return { thinking: "Error occurred", actions: [], done: true, message: "Sorry, I encountered an error." };
  }
}

module.exports = { analyzeScreenshot, chatWithScreenshot, agentAction, summarizeActivity, extractTopics, transcribeAudio };

async function transcribeAudio(base64Audio) {
  try {
    const config = getConfig();
    if (!config.apiKey) return null;

    const ai = new GoogleGenAI({ apiKey: config.apiKey });

    const modelResponse = await ai.models.generateContent({
      model: config.model,
      contents: [{
        role: 'user',
        parts: [
          { text: 'Transcribe the following audio to text. Return ONLY the transcribed text, nothing else. If the audio is empty or unintelligible, return an empty string.' },
          { inlineData: { mimeType: 'audio/webm', data: base64Audio } }
        ]
      }]
    });

    return modelResponse.text.trim();
  } catch (error) {
    console.error("Error transcribing audio:", error);
    return null;
  }
}

async function extractTopics(entries) {
  try {
    const config = getConfig();
    if (!config.apiKey) return { nodes: [], edges: [] };

    const ai = new GoogleGenAI({ apiKey: config.apiKey });

    const entrySummaries = entries.map(e => {
      const text = e.summary || e.message || e.task || '';
      return `[${e.time}] (${e.type}) ${text}`;
    }).join('\n');

    const prompt = `Analyze these daily activity journal entries and extract a knowledge graph of key topics, apps, and activities.

Journal entries:
${entrySummaries}

Return ONLY valid JSON in this exact format:
{
  "nodes": [
    { "id": "spotify", "label": "Spotify", "type": "app" },
    { "id": "music", "label": "Music", "type": "topic" },
    { "id": "writing", "label": "Writing", "type": "action" }
  ],
  "edges": [
    { "source": "spotify", "target": "music", "label": "listening" }
  ]
}

Node types: "app", "topic", "action", "person"
Rules:
- Extract 5-15 nodes maximum
- Connect related nodes with meaningful edges
- Use short labels (1-3 words)
- Return ONLY the JSON, no markdown`;

    const modelResponse = await ai.models.generateContent({
      model: config.model,
      contents: [{ role: 'user', parts: [{ text: prompt }] }]
    });

    let text = modelResponse.text.trim();
    if (text.startsWith('```json')) text = text.substring(7);
    if (text.startsWith('```')) text = text.substring(3);
    if (text.endsWith('```')) text = text.substring(0, text.length - 3);

    return JSON.parse(text.trim());
  } catch (error) {
    console.error("Error extracting topics:", error);
    return { nodes: [], edges: [] };
  }
}

async function summarizeActivity(dataUrl) {
  try {
    const config = getConfig();
    if (!config.apiKey) return null;

    const ai = new GoogleGenAI({ apiKey: config.apiKey });
    const mimeTypeStr = dataUrl.split(';')[0];
    const mimeType = mimeTypeStr.split(':')[1];
    const base64Data = dataUrl.split(',')[1];

    const prompt = `Describe what the user is doing on their screen in ONE short sentence (max 15 words). Be specific about the app and action. Examples: "Browsing playlists on Spotify", "Editing a document in Google Docs", "Reading emails in Gmail". Return ONLY the sentence, nothing else.`;

    const modelResponse = await ai.models.generateContent({
      model: config.model,
      contents: [
        { role: 'user', parts: [
            { text: prompt },
            { inlineData: { mimeType, data: base64Data } }
          ]
        }
      ]
    });

    return modelResponse.text.trim();
  } catch (error) {
    console.error("Error summarizing activity:", error);
    return null;
  }
}
