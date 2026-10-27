//! Settings are owned by the frontend's store (it knows every key and default),
//! but persisted here as JSON so the background alert monitor can read the
//! location and notification preferences even while the window is hidden.

use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::{json, Value};

use crate::error::Result;

pub struct Settings {
    path: PathBuf,
    value: Mutex<Value>,
}

impl Settings {
    pub fn load(dir: PathBuf) -> Self {
        let path = dir.join("settings.json");
        let value = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_else(|| json!({}));
        Settings { path, value: Mutex::new(value) }
    }

    pub fn get(&self) -> Value {
        self.value.lock().unwrap().clone()
    }

    pub fn save(&self, v: Value) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Write-then-rename so a crash mid-write can't corrupt the file.
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&v)?)?;
        std::fs::rename(&tmp, &self.path)?;
        *self.value.lock().unwrap() = v;
        Ok(())
    }

    pub fn bool(&self, key: &str, default: bool) -> bool {
        self.value.lock().unwrap()[key].as_bool().unwrap_or(default)
    }

    pub fn location(&self) -> Option<(f64, f64, String)> {
        let v = self.value.lock().unwrap();
        let loc = &v["location"];
        Some((loc["lat"].as_f64()?, loc["lon"].as_f64()?, loc["name"].as_str().unwrap_or("your area").to_string()))
    }
}
