//! Streams chat completions from a local model server. The web StormView ran a
//! model in-browser with WebLLM; desktop webviews (WebKitGTK especially) can't
//! rely on WebGPU, so the desktop app talks to Ollama or any OpenAI-compatible
//! server (LM Studio, llama.cpp, vLLM…) running on the user's machine instead.

use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::ipc::Channel;

use crate::error::{Error, Result};
use crate::http::get_json;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
pub enum ChatEvent {
    Delta { text: String },
    Done,
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
    channel: Channel<ChatEvent>,
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
                let _ = channel.send(ChatEvent::Done);
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
                let _ = channel.send(ChatEvent::Delta { text: text.to_string() });
            }
            if ollama && v["done"].as_bool() == Some(true) {
                let _ = channel.send(ChatEvent::Done);
                return Ok(());
            }
        }
    }
    let _ = channel.send(ChatEvent::Done);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Needs a local Ollama with llama3.2:3b; run with `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn ollama_streams() {
        let c = crate::http::client();
        let models = list_models(&c, "ollama", "http://localhost:11434").await.expect("ollama running");
        println!("models: {models:?}");
        let out = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = out.clone();
        let channel: Channel<ChatEvent> = Channel::new(move |body| {
            if let tauri::ipc::InvokeResponseBody::Json(s) = body {
                sink.lock().unwrap().push(s);
            }
            Ok(())
        });
        let msgs = vec![ChatMessage { role: "user".into(), content: "Reply with just the word: sunny".into() }];
        chat(&c, "ollama", "http://localhost:11434", &models[0], msgs, channel).await.expect("chat");
        let events = out.lock().unwrap();
        println!("{} events, last: {:?}", events.len(), events.last());
        assert!(events.iter().any(|e| e.contains("delta")) && events.last().unwrap().contains("done"));
    }
}
