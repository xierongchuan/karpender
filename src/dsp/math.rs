use super::DEFAULT_SAMPLE_RATE;

pub(super) fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);

    value * value * (3.0 - 2.0 * value)
}

pub(super) fn effective_privacy(value: f32) -> f32 {
    value.clamp(0.0, 1.0).powf(0.78)
}

pub(super) fn soft_clip(sample: f32) -> f32 {
    sample / (1.0 + sample.abs() * 0.35)
}

pub(super) fn input_drive(gain: f32) -> f32 {
    let gain = gain.clamp(0.0, 4.0);
    if gain <= 1.0 {
        gain
    } else {
        1.0 + (gain - 1.0).sqrt() * 0.72
    }
}

/// One pole smoothing coefficient for a time constant in seconds.
pub(super) fn one_pole_coeff(seconds: f32) -> f32 {
    let samples = (seconds.max(0.000_02) * DEFAULT_SAMPLE_RATE as f32).max(1.0);

    (1.0 / samples).clamp(0.000_01, 1.0)
}

pub(super) fn amplitude_to_db(amplitude: f32) -> f32 {
    20.0 * (amplitude.max(1.0e-6)).log10()
}

/// Geometric interpolation between voice ratios, so blending pitch or formant
/// factors stays musical instead of drifting toward the arithmetic mean.
pub(super) fn ratio_blend(from: f32, to: f32, amount: f32) -> f32 {
    let from = from.max(0.05);
    let to = to.max(0.05);

    (from.ln() + (to.ln() - from.ln()) * amount.clamp(0.0, 1.0)).exp()
}
