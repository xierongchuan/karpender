use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub input_node_id: Option<u32>,
    pub gain: f32,
    pub noise_gate: f32,
    pub robot_amount: f32,
    pub monotone: bool,
    #[serde(default)]
    pub monitor_output: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            input_node_id: None,
            gain: 1.0,
            noise_gate: 0.03,
            robot_amount: 0.55,
            monotone: false,
            monitor_output: false,
        }
    }
}

pub fn load() -> AppConfig {
    let Some(path) = config_path() else {
        return AppConfig::default();
    };

    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save(config: &AppConfig) {
    let Some(path) = config_path() else {
        return;
    };

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(raw) = serde_json::to_string_pretty(config) {
        let _ = fs::write(path, raw);
    }
}

fn config_path() -> Option<PathBuf> {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;

    Some(config_home.join("karpender").join("settings.json"))
}
