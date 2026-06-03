use crate::config::{AppConfig, VoiceMode, VoiceProfile};

pub const MANUAL_PROFILE_INDEX: usize = 0;

struct BuiltInProfile {
    id: &'static str,
    name: &'static str,
    gain: f32,
    noise_gate: f32,
    robot_amount: f32,
    monotone: bool,
    voice_mode: VoiceMode,
}

impl BuiltInProfile {
    fn to_profile(&self) -> VoiceProfile {
        VoiceProfile {
            id: self.id.to_string(),
            name: self.name.to_string(),
            gain: self.gain,
            noise_gate: self.noise_gate,
            robot_amount: self.robot_amount,
            monotone: self.monotone,
            voice_mode: self.voice_mode,
        }
    }
}

const BUILT_IN_PROFILES: &[BuiltInProfile] = &[
    BuiltInProfile {
        id: "balanced-mask",
        name: "Balanced Mask",
        gain: 1.25,
        noise_gate: 0.06,
        robot_amount: 0.62,
        monotone: false,
        voice_mode: VoiceMode::Masked,
    },
    BuiltInProfile {
        id: "deep-morph",
        name: "Deep Morph",
        gain: 1.7,
        noise_gate: 0.1,
        robot_amount: 0.94,
        monotone: true,
        voice_mode: VoiceMode::DeepMorph,
    },
    BuiltInProfile {
        id: "cinematic-high",
        name: "Cinematic High",
        gain: 1.45,
        noise_gate: 0.08,
        robot_amount: 0.98,
        monotone: true,
        voice_mode: VoiceMode::CinematicHigh,
    },
    BuiltInProfile {
        id: "strong-privacy",
        name: "Strong Privacy",
        gain: 2.2,
        noise_gate: 0.16,
        robot_amount: 0.95,
        monotone: true,
        voice_mode: VoiceMode::Masked,
    },
    BuiltInProfile {
        id: "bright-stranger",
        name: "Bright Stranger",
        gain: 1.45,
        noise_gate: 0.08,
        robot_amount: 0.78,
        monotone: false,
        voice_mode: VoiceMode::BrightStranger,
    },
];

pub fn labels(config: &AppConfig) -> Vec<String> {
    let mut labels = Vec::with_capacity(1 + BUILT_IN_PROFILES.len() + config.profiles.len());
    labels.push("Manual Settings".to_string());
    labels.extend(
        BUILT_IN_PROFILES
            .iter()
            .map(|profile| profile.name.to_string()),
    );
    labels.extend(config.profiles.iter().map(|profile| profile.name.clone()));
    labels
}

pub fn selected_index(config: &AppConfig) -> usize {
    let Some(active_id) = config.active_profile_id.as_deref() else {
        return MANUAL_PROFILE_INDEX;
    };

    if let Some(index) = BUILT_IN_PROFILES
        .iter()
        .position(|profile| profile.id == active_id)
    {
        return index + 1;
    }

    config
        .profiles
        .iter()
        .position(|profile| profile.id.as_str() == active_id)
        .map(|index| index + 1 + BUILT_IN_PROFILES.len())
        .unwrap_or(MANUAL_PROFILE_INDEX)
}

pub fn profile_for_index(config: &AppConfig, selected: usize) -> Option<VoiceProfile> {
    let built_in_index = selected.checked_sub(1)?;
    if let Some(profile) = BUILT_IN_PROFILES.get(built_in_index) {
        return Some(profile.to_profile());
    }

    let custom_index = built_in_index - BUILT_IN_PROFILES.len();
    config.profiles.get(custom_index).cloned()
}

pub fn custom_index(config: &AppConfig, selected: usize) -> Option<usize> {
    selected
        .checked_sub(BUILT_IN_PROFILES.len() + 1)
        .filter(|index| *index < config.profiles.len())
}

pub fn next_custom_number(config: &AppConfig) -> usize {
    (1..)
        .find(|number| {
            let id = format!("custom-profile-{number}");
            !config.profiles.iter().any(|profile| profile.id == id)
        })
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_profiles_are_indexed_after_manual_and_built_ins() {
        let mut config = AppConfig::default();
        config.profiles.push(config.profile_from_current(
            "custom-profile-1".to_string(),
            "Custom Profile 1".to_string(),
        ));

        let index = BUILT_IN_PROFILES.len() + 1;

        assert_eq!(custom_index(&config, index), Some(0));
        assert_eq!(
            profile_for_index(&config, index)
                .map(|profile| profile.id)
                .as_deref(),
            Some("custom-profile-1")
        );
    }

    #[test]
    fn selected_index_falls_back_to_manual_for_unknown_active_profile() {
        let config = AppConfig {
            active_profile_id: Some("missing-profile".to_string()),
            ..AppConfig::default()
        };

        assert_eq!(selected_index(&config), MANUAL_PROFILE_INDEX);
    }

    #[test]
    fn built_in_profile_labels_keep_dropdown_order() {
        let labels = labels(&AppConfig::default());

        assert_eq!(
            labels,
            [
                "Manual Settings",
                "Balanced Mask",
                "Deep Morph",
                "Cinematic High",
                "Strong Privacy",
                "Bright Stranger",
            ]
        );
    }

    #[test]
    fn built_in_profile_lookup_keeps_original_indexes() {
        let config = AppConfig::default();

        assert_eq!(
            profile_for_index(&config, 1).map(|profile| profile.id),
            Some("balanced-mask".to_string())
        );
        assert_eq!(
            profile_for_index(&config, 5).map(|profile| profile.id),
            Some("bright-stranger".to_string())
        );
    }
}
