use crate::config::VoiceMode;

/// Target voice of a mode. Pitch and formants are independent here, which is
/// what keeps the modes from all collapsing into the same thin, squeaky timbre:
/// a high target pitch with unchanged formants sounds nothing like a high pitch
/// with a small vocal tract.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct VoiceRecipe {
    /// Fundamental the speaker is normalised to, in Hz.
    pub(super) target_f0: f32,
    /// Vocal tract scaling. Below 1.0 sounds larger, above 1.0 smaller.
    pub(super) formant_ratio: f32,
    /// How strongly the mode insists on its target pitch.
    pub(super) normalize: f32,
    /// Spectral tilt in dB across the speech range, positive is brighter.
    pub(super) tilt_db: f32,
    /// Amount of breath noise mixed back in with voiced speech.
    pub(super) breath: f32,
    /// Output level the loudness stabiliser aims for.
    pub(super) output_level: f32,
}

pub(super) fn voice_recipe(mode: VoiceMode) -> VoiceRecipe {
    match mode {
        VoiceMode::Masked => VoiceRecipe {
            target_f0: 150.0,
            formant_ratio: 1.00,
            normalize: 0.85,
            tilt_db: 0.0,
            breath: 0.035,
            output_level: 0.22,
        },
        VoiceMode::BrightStranger => VoiceRecipe {
            target_f0: 178.0,
            formant_ratio: 1.09,
            normalize: 0.90,
            tilt_db: 1.8,
            breath: 0.050,
            output_level: 0.22,
        },
        VoiceMode::DeepMorph => VoiceRecipe {
            target_f0: 108.0,
            formant_ratio: 0.87,
            normalize: 0.92,
            tilt_db: -2.2,
            breath: 0.030,
            output_level: 0.23,
        },
        VoiceMode::CinematicHigh => VoiceRecipe {
            target_f0: 205.0,
            formant_ratio: 1.16,
            normalize: 0.95,
            tilt_db: 2.6,
            breath: 0.065,
            output_level: 0.21,
        },
        VoiceMode::WarmNeighbour => VoiceRecipe {
            target_f0: 132.0,
            formant_ratio: 0.94,
            normalize: 0.72,
            tilt_db: -1.0,
            breath: 0.040,
            output_level: 0.23,
        },
        VoiceMode::CalmAndrogynous => VoiceRecipe {
            target_f0: 158.0,
            formant_ratio: 1.03,
            normalize: 1.00,
            tilt_db: 0.4,
            breath: 0.045,
            output_level: 0.22,
        },
        VoiceMode::SoftAlto => VoiceRecipe {
            target_f0: 192.0,
            formant_ratio: 1.12,
            normalize: 0.95,
            tilt_db: 1.2,
            breath: 0.075,
            output_level: 0.21,
        },
        VoiceMode::LowBaritone => VoiceRecipe {
            target_f0: 96.0,
            formant_ratio: 0.84,
            normalize: 0.95,
            tilt_db: -3.0,
            breath: 0.028,
            output_level: 0.24,
        },
    }
}
