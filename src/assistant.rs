//! Streams chat completions from a local model server — Ollama or any
//! OpenAI-compatible server (LM Studio, llama.cpp, vLLM…) on the user's machine —
//! replacing the web app's in-browser WebLLM model.

use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{Error, Result};
use crate::http::get_json;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

fn base(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

pub async fn list_models(client: &Client, provider: &str, base_url: &str) -> Result<Vec<String>> {
    let b = base(base_url);
    if provider == "ollama" {
        let v: Value = get_json(client, &format!("{b}/api/tags"), None).await?;
        Ok(v["models"]
            .as_array()
            .map(|a| a.iter().filter_map(|m| m["name"].as_str().map(String::from)).collect())
            .unwrap_or_default())
    } else {
        let v: Value = get_json(client, &format!("{b}/v1/models"), None).await?;
        Ok(v["data"]
            .as_array()
            .map(|a| a.iter().filter_map(|m| m["id"].as_str().map(String::from)).collect())
            .unwrap_or_default())
    }
}

pub async fn chat(
    client: &Client,
    provider: &str,
    base_url: &str,
    model: &str,
    messages: Vec<ChatMessage>,
    on_delta: impl Fn(String),
) -> Result<()> {
    let b = base(base_url);
    let ollama = provider == "ollama";
    let (url, body) = if ollama {
        (format!("{b}/api/chat"), json!({ "model": model, "messages": messages, "stream": true }))
    } else {
        (format!("{b}/v1/chat/completions"), json!({ "model": model, "messages": messages, "stream": true }))
    };
    let res = client
        .post(&url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(600))
        .send()
        .await
        .map_err(|e| Error::msg(format!("Couldn't reach the model server at {b} — is it running? ({e})")))?;
    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        return Err(Error::msg(format!("Model server returned {status}: {}", text.chars().take(200).collect::<String>())));
    }

    // Ollama streams NDJSON; OpenAI-compatible servers stream SSE `data:` lines.
    let mut stream = res.bytes_stream();
    let mut buf = String::new();
    while let Some(chunk) = stream.next().await {
        buf.push_str(&String::from_utf8_lossy(&chunk?));
        while let Some(nl) = buf.find('\n') {
            let line: String = buf.drain(..=nl).collect();
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let payload = if ollama { line } else { line.strip_prefix("data:").map(str::trim).unwrap_or("") };
            if payload == "[DONE]" {
                return Ok(());
            }
            let Ok(v) = serde_json::from_str::<Value>(payload) else { continue };
            if let Some(err) = v["error"].as_str() {
                return Err(Error::msg(err.to_string()));
            }
            let delta = if ollama {
                v["message"]["content"].as_str()
            } else {
                v["choices"][0]["delta"]["content"].as_str()
            };
            if let Some(text) = delta.filter(|t| !t.is_empty()) {
                on_delta(text.to_string());
            }
            if ollama && v["done"].as_bool() == Some(true) {
                return Ok(());
            }
        }
    }
    Ok(())
}
