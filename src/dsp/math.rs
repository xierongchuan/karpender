use super::{PITCH_WINDOW, TWO_PI};

pub(super) fn raised_cosine(phase: f32) -> f32 {
    0.5 - 0.5 * (TWO_PI * phase).cos()
}

pub(super) fn advance_pitch_phase(ratio: f32, phase: f32) -> f32 {
    let step = (ratio - 1.0).abs().max(0.08) / PITCH_WINDOW as f32;
    (phase + step).fract()
}

pub(super) fn smoothstep(value: f32) -> f32 {
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
