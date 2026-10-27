use std::time::Duration;

use reqwest::{Client, StatusCode};
use serde::de::DeserializeOwned;

use crate::error::{Error, Result};

/// NWS and Nominatim both require an identifying User-Agent; browsers can't set
/// one, which is a big reason the network layer lives in Rust.
pub const USER_AGENT: &str =
    "StormView-Desktop/1.0 (+https://github.com/arc360alt/StormView-Rewrite)";

pub fn client() -> Client {
    Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .gzip(true)
        .brotli(true)
        .build()
        .expect("failed to build HTTP client")
}

/// GET a URL and decode the JSON body, turning non-2xx responses into a readable error.
pub async fn get_json<T: DeserializeOwned>(client: &Client, url: &str, accept: Option<&str>) -> Result<T> {
    let mut req = client.get(url);
    if let Some(a) = accept {
        req = req.header("Accept", a);
    }
    let res = req.send().await?;
    let status = res.status();
    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(status_error(status, &body));
    }
    let bytes = res.bytes().await?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn status_error(status: StatusCode, body: &str) -> Error {
    let snippet: String = body.chars().take(140).collect();
    Error::msg(format!("HTTP {}: {}", status.as_u16(), snippet.trim()))
}
