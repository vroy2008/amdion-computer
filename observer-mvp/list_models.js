const { GoogleGenAI } = require('@google/genai');
require('dotenv').config();

async function listModels() {
  const ai = new GoogleGenAI({});
  try {
    const response = await ai.models.list(); // Or however it is in this SDK version
    // Usually it's an iterable or array
    let count = 0;
    for await (const model of response) {
      console.log(`- ${model.name}`);
      count++;
    }
    if (count === 0) console.log("No models returned.");
  } catch(e) {
    if (e.message && e.message.includes("is not fully supported")) {
        // Fallback to manual fetch
        const apiKey = process.env.GEMINI_API_KEY;
        const res = await fetch(`https://generativelanguage.googleapis.com/v1beta/models?key=${apiKey}`);
        const data = await res.json();
        if (data.models) {
            data.models.forEach(m => console.log(`- ${m.name}`));
        } else {
            console.log(data);
        }
    } else {
        console.error("Error listing models:", e);
    }
  }
}

listModels();
