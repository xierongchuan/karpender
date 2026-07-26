use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, io::ErrorKind, path::PathBuf};

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
    WarmNeighbour,
    CalmAndrogynous,
    SoftAlto,
    LowBaritone,
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

impl VoiceMode {
    pub const ALL: [Self; 8] = [
        Self::Masked,
        Self::BrightStranger,
        Self::DeepMorph,
        Self::CinematicHigh,
        Self::WarmNeighbour,
        Self::CalmAndrogynous,
        Self::SoftAlto,
        Self::LowBaritone,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Masked => "Masked Voice",
            Self::BrightStranger => "Bright Stranger",
            Self::DeepMorph => "Deep Morph",
            Self::CinematicHigh => "Cinematic High",
            Self::WarmNeighbour => "Warm Neighbour",
            Self::CalmAndrogynous => "Calm Androgynous",
            Self::SoftAlto => "Soft Alto",
            Self::LowBaritone => "Low Baritone",
        }
    }

    /// Short hint shown next to the mode picker so the modes are actually
    /// distinguishable before trying them.
    pub fn description(self) -> &'static str {
        match self {
            Self::Masked => "Neutral mid pitch, unchanged formants",
            Self::BrightStranger => "Higher pitch with a brighter, smaller head",
            Self::DeepMorph => "Low pitch with a larger, darker head",
            Self::CinematicHigh => "High pitch, wide open and airy",
            Self::WarmNeighbour => "Low mid pitch, warm and close",
            Self::CalmAndrogynous => "Mid pitch, deliberately gender neutral",
            Self::SoftAlto => "High mid pitch, soft and breathy",
            Self::LowBaritone => "Lowest pitch, full chest resonance",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|mode| *mode == self)
            .unwrap_or_default()
    }

    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or_default()
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

pub fn load() -> Result<AppConfig> {
    let Some(path) = config_path() else {
        return Ok(AppConfig::default());
    };

    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(AppConfig::default()),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("failed to read settings from {}", path.as_path().display())
            });
        }
    };

    serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse settings from {}", path.as_path().display()))
}

pub fn save(config: &AppConfig) -> Result<()> {
    let Some(path) = config_path() else {
        return Ok(());
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create settings directory {}", parent.display()))?;
    }

    let raw = serde_json::to_string_pretty(config).context("failed to serialize settings")?;
    fs::write(&path, raw)
        .with_context(|| format!("failed to write settings to {}", path.as_path().display()))
}

fn config_path() -> Option<PathBuf> {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;

    Some(config_home.join("karpender").join("settings.json"))
}
