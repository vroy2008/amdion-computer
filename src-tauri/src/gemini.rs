// Gemini API client. Isolated so the LLM provider can be swapped later.

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum GeminiPart {
    Text { text: String },
    InlineData { #[serde(rename = "inlineData")] inline_data: InlineData },
}

#[derive(Debug, Serialize, Deserialize)]
struct InlineData {
    #[serde(rename = "mimeType")]
    mime_type: String,
    data: String,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiCandidateContent>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidateContent {
    parts: Option<Vec<GeminiResponsePart>>,
}

#[derive(Debug, Deserialize)]
struct GeminiResponsePart {
    text: Option<String>,
}

fn extract_text(body: GeminiResponse) -> Result<String, String> {
    body.candidates
        .and_then(|c| c.into_iter().next())
        .and_then(|c| c.content)
        .and_then(|c| c.parts)
        .and_then(|p| p.into_iter().next())
        .and_then(|p| p.text)
        .ok_or_else(|| "Empty response from Gemini".into())
}

pub async fn call_gemini(api_key: &str, model: &str, prompt: &str) -> Result<String, String> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let request = GeminiRequest {
        contents: vec![GeminiContent {
            role: "user".into(),
            parts: vec![GeminiPart::Text { text: prompt.into() }],
        }],
    };

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let body: GeminiResponse = resp
        .json()
        .await
        .map_err(|e| format!("Parse failed: {}", e))?;

    extract_text(body)
}

pub async fn call_gemini_with_image(
    api_key: &str,
    model: &str,
    prompt: &str,
    image_base64: &str,
    mime_type: &str,
) -> Result<String, String> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let request = GeminiRequest {
        contents: vec![GeminiContent {
            role: "user".into(),
            parts: vec![
                GeminiPart::Text { text: prompt.into() },
                GeminiPart::InlineData {
                    inline_data: InlineData {
                        mime_type: mime_type.into(),
                        data: image_base64.into(),
                    },
                },
            ],
        }],
    };

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let body: GeminiResponse = resp
        .json()
        .await
        .map_err(|e| format!("Parse failed: {}", e))?;

    extract_text(body)
}
