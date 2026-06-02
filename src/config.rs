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
    pub voice_mode: VoiceMode,
    #[serde(default)]
    pub monitor_output: bool,
    #[serde(default)]
    pub active_profile_id: Option<String>,
    #[serde(default)]
    pub profiles: Vec<VoiceProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceProfile {
    pub id: String,
    pub name: String,
    pub gain: f32,
    pub noise_gate: f32,
    pub robot_amount: f32,
    pub monotone: bool,
    #[serde(default)]
    pub voice_mode: VoiceMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum VoiceMode {
    #[default]
    Masked,
    BrightStranger,
    DeepMorph,
    CinematicHigh,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            input_node_id: None,
            gain: 1.0,
            noise_gate: 0.03,
            robot_amount: 0.55,
            monotone: false,
            voice_mode: VoiceMode::Masked,
            monitor_output: false,
            active_profile_id: None,
            profiles: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn apply_profile(&mut self, profile: &VoiceProfile) {
        self.gain = profile.gain;
        self.noise_gate = profile.noise_gate;
        self.robot_amount = profile.robot_amount;
        self.monotone = profile.monotone;
        self.voice_mode = profile.voice_mode;
        self.active_profile_id = Some(profile.id.clone());
    }

    pub fn profile_from_current(&self, id: String, name: String) -> VoiceProfile {
        VoiceProfile {
            id,
            name,
            gain: self.gain,
            noise_gate: self.noise_gate,
            robot_amount: self.robot_amount,
            monotone: self.monotone,
            voice_mode: self.voice_mode,
        }
    }
}

pub fn built_in_profiles() -> Vec<VoiceProfile> {
    vec![
        VoiceProfile {
            id: "balanced-mask".to_string(),
            name: "Balanced Mask".to_string(),
            gain: 1.25,
            noise_gate: 0.06,
            robot_amount: 0.62,
            monotone: false,
            voice_mode: VoiceMode::Masked,
        },
        VoiceProfile {
            id: "deep-morph".to_string(),
            name: "Deep Morph".to_string(),
            gain: 1.7,
            noise_gate: 0.1,
            robot_amount: 0.94,
            monotone: true,
            voice_mode: VoiceMode::DeepMorph,
        },
        VoiceProfile {
            id: "cinematic-high".to_string(),
            name: "Cinematic High".to_string(),
            gain: 1.45,
            noise_gate: 0.08,
            robot_amount: 0.98,
            monotone: true,
            voice_mode: VoiceMode::CinematicHigh,
        },
        VoiceProfile {
            id: "strong-privacy".to_string(),
            name: "Strong Privacy".to_string(),
            gain: 2.2,
            noise_gate: 0.16,
            robot_amount: 0.95,
            monotone: true,
            voice_mode: VoiceMode::Masked,
        },
        VoiceProfile {
            id: "bright-stranger".to_string(),
            name: "Bright Stranger".to_string(),
            gain: 1.45,
            noise_gate: 0.08,
            robot_amount: 0.78,
            monotone: false,
            voice_mode: VoiceMode::BrightStranger,
        },
    ]
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
