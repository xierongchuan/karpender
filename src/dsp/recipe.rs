use crate::config::VoiceMode;

pub(super) fn pitch_recipe(
    mode: VoiceMode,
    privacy: f32,
    timbre_jitter: f32,
    accent: f32,
) -> (f32, f32, f32, f32) {
    match mode {
        VoiceMode::Masked => {
            let down_ratio = 0.94 - privacy * 0.18 + timbre_jitter * 0.010 + accent * 0.004;
            let up_ratio = 1.025 + privacy * 0.13 + timbre_jitter * 0.018 + accent * 0.006;
            let down_weight = 0.72 - privacy * 0.22;
            let pitch_mix = (privacy * 0.78).min(0.84);

            (down_ratio, up_ratio, down_weight, pitch_mix)
        }
        VoiceMode::BrightStranger => {
            let down_ratio = 1.0 - privacy * 0.035 + timbre_jitter * 0.012 + accent * 0.006;
            let up_ratio = 1.13 + privacy * 0.10 + timbre_jitter * 0.035 + accent * 0.015;
            let down_weight = 0.20 - privacy * 0.07;
            let pitch_mix = (0.56 + privacy * 0.30).min(0.88);

            (down_ratio, up_ratio, down_weight, pitch_mix)
        }
        VoiceMode::DeepMorph => {
            let down_ratio = 0.82 - privacy * 0.07 + timbre_jitter * 0.018 + accent * 0.010;
            let up_ratio = 1.18 + privacy * 0.16 + timbre_jitter * 0.050 + accent * 0.020;
            let down_weight = 0.38 - privacy * 0.12;
            let pitch_mix = (0.72 + privacy * 0.22).min(0.94);

            (down_ratio, up_ratio, down_weight, pitch_mix)
        }
        VoiceMode::CinematicHigh => {
            let down_ratio = 1.04 - privacy * 0.025 + timbre_jitter * 0.012 + accent * 0.008;
            let up_ratio = 1.34 + privacy * 0.12 + timbre_jitter * 0.060 + accent * 0.026;
            let down_weight = 0.055;
            let pitch_mix = (0.88 + privacy * 0.09).min(0.97);

            (down_ratio, up_ratio, down_weight, pitch_mix)
        }
    }
}
