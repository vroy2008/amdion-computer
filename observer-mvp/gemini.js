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

module.exports = { analyzeScreenshot, chatWithScreenshot };
