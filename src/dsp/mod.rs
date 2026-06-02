use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use atomic_float::AtomicF32;

pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;
const TWO_PI: f32 = std::f32::consts::PI * 2.0;
const PITCH_WINDOW: usize = 1536;
const PITCH_BUFFER: usize = PITCH_WINDOW * 4;
const MIN_EXPANDER_GAIN: f32 = 0.22;

#[derive(Debug, Default)]
pub struct SharedDspParams {
    gain: AtomicF32,
    noise_gate: AtomicF32,
    robot_amount: AtomicF32,
    monotone: AtomicBool,
}

impl SharedDspParams {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            gain: AtomicF32::new(1.0),
            noise_gate: AtomicF32::new(0.03),
            robot_amount: AtomicF32::new(0.55),
            monotone: AtomicBool::new(false),
        })
    }

    pub fn set_gain(&self, value: f32) {
        self.gain.store(value.clamp(0.0, 4.0), Ordering::Relaxed);
    }

    pub fn set_noise_gate(&self, value: f32) {
        self.noise_gate
            .store(value.clamp(0.0, 1.0), Ordering::Relaxed);
    }

    pub fn set_robot_amount(&self, value: f32) {
        self.robot_amount
            .store(value.clamp(0.0, 1.0), Ordering::Relaxed);
    }

    pub fn set_monotone(&self, enabled: bool) {
        self.monotone.store(enabled, Ordering::Relaxed);
    }

    fn snapshot(&self) -> DspSnapshot {
        DspSnapshot {
            gain: self.gain.load(Ordering::Relaxed),
            noise_gate: self.noise_gate.load(Ordering::Relaxed),
            robot_amount: self.robot_amount.load(Ordering::Relaxed),
            monotone: self.monotone.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct DspSnapshot {
    gain: f32,
    noise_gate: f32,
    robot_amount: f32,
    monotone: bool,
}

#[derive(Debug)]
pub struct VoiceProcessor {
    pitch_buffer: Vec<f32>,
    write_pos: usize,
    down_phase: f32,
    up_phase: f32,
    body_state: f32,
    speech_state: f32,
    prosody_state: f32,
    expander_envelope: f32,
    expander_gain: f32,
    compressor_envelope: f32,
    doubler_phase: f32,
}

impl Default for VoiceProcessor {
    fn default() -> Self {
        Self {
            pitch_buffer: vec![0.0; PITCH_BUFFER],
            write_pos: 0,
            down_phase: 0.0,
            up_phase: 0.5,
            body_state: 0.0,
            speech_state: 0.0,
            prosody_state: 0.0,
            expander_envelope: 0.0,
            expander_gain: 1.0,
            compressor_envelope: 0.0,
            doubler_phase: 0.0,
        }
    }
}

impl VoiceProcessor {
    pub fn process_sample(&mut self, input: f32, params: &SharedDspParams) -> f32 {
        let params = params.snapshot();
        let privacy = effective_privacy(params.robot_amount);
        let monotone_privacy = if params.monotone {
            privacy.max(0.35)
        } else {
            privacy
        };
        let cleaned = self.noise_cleanup(input * params.gain, params.noise_gate);

        let down_ratio = 0.93 - monotone_privacy * 0.13;
        let up_ratio = 1.035 + monotone_privacy * 0.075;
        self.pitch_buffer[self.write_pos] = cleaned;
        let shifted_down = self.pitch_tap_pair(down_ratio, self.down_phase);
        let shifted_up = self.pitch_tap_pair(up_ratio, self.up_phase);
        self.down_phase = advance_pitch_phase(down_ratio, self.down_phase);
        self.up_phase = advance_pitch_phase(up_ratio, self.up_phase);
        self.write_pos = (self.write_pos + 1) % self.pitch_buffer.len();

        let shifted = shifted_down * 0.84 + shifted_up * 0.16;
        let pitch_mix = (monotone_privacy * 0.72).min(0.78);
        let pitched = cleaned * (1.0 - pitch_mix) + shifted * pitch_mix;
        let disguised = self.identity_mask(pitched, monotone_privacy);
        let mut sample = self.speech_compress(disguised, monotone_privacy);
        sample = self.flatten_personality(sample, monotone_privacy * 0.22);

        if params.monotone {
            sample = self.flatten_personality(sample, 0.32 + monotone_privacy * 0.34);
        }

        soft_clip(sample).clamp(-1.0, 1.0)
    }

    fn noise_cleanup(&mut self, input: f32, threshold: f32) -> f32 {
        let threshold = threshold.clamp(0.0, 0.35);
        if threshold <= 0.001 {
            self.expander_gain += 0.08 * (1.0 - self.expander_gain);
            return input * self.expander_gain;
        }

        let amplitude = input.abs();
        let coeff = if amplitude > self.expander_envelope {
            0.12
        } else {
            0.006
        };
        self.expander_envelope += coeff * (amplitude - self.expander_envelope);

        let floor = threshold * 0.45;
        let open = threshold * 1.85 + 0.002;
        let ratio = ((self.expander_envelope - floor) / (open - floor)).clamp(0.0, 1.0);
        let target_gain = MIN_EXPANDER_GAIN + (1.0 - MIN_EXPANDER_GAIN) * smoothstep(ratio);
        let gain_coeff = if target_gain > self.expander_gain {
            0.08
        } else {
            0.018
        };
        self.expander_gain += gain_coeff * (target_gain - self.expander_gain);

        input * self.expander_gain
    }

    fn pitch_tap_pair(&self, ratio: f32, phase: f32) -> f32 {
        let first = self.pitch_tap(ratio, phase);
        let second_phase = (phase + 0.5).fract();
        let second = self.pitch_tap(ratio, second_phase);
        let fade = raised_cosine(phase);

        first * fade + second * (1.0 - fade)
    }

    fn pitch_tap(&self, ratio: f32, phase: f32) -> f32 {
        let delay = if ratio < 1.0 {
            phase * PITCH_WINDOW as f32
        } else {
            (1.0 - phase) * PITCH_WINDOW as f32
        } + 64.0;

        self.read_delay(delay)
    }

    fn read_delay(&self, delay: f32) -> f32 {
        let len = self.pitch_buffer.len() as f32;
        let read = (self.write_pos as f32 - delay).rem_euclid(len);
        let floor = read.floor();
        let i0 = floor as usize % self.pitch_buffer.len();
        let i1 = (i0 + 1) % self.pitch_buffer.len();
        let frac = read - floor;

        self.pitch_buffer[i0] * (1.0 - frac) + self.pitch_buffer[i1] * frac
    }

    fn identity_mask(&mut self, input: f32, privacy: f32) -> f32 {
        self.body_state += 0.035 * (input - self.body_state);
        self.speech_state += 0.16 * (input - self.speech_state);
        let body = self.body_state;
        let mids = self.speech_state - self.body_state;
        let consonants = input - self.speech_state;
        let short_delay = self.read_delay(380.0 + privacy * 260.0);
        self.doubler_phase += TWO_PI * 0.37 / DEFAULT_SAMPLE_RATE as f32;
        if self.doubler_phase >= TWO_PI {
            self.doubler_phase -= TWO_PI;
        }
        let moving_delay = self.read_delay(660.0 + self.doubler_phase.sin() * 90.0);
        let doubler = (short_delay * 0.65 + moving_delay * 0.35) * (0.22 + privacy * 0.28);

        let masked = body * (0.72 - privacy * 0.12)
            + mids * (0.50 - privacy * 0.20)
            + consonants * (1.16 + privacy * 0.22)
            + doubler;
        let mix = (privacy * 0.82).min(0.86);

        input * (1.0 - mix) + masked * mix
    }

    fn speech_compress(&mut self, input: f32, privacy: f32) -> f32 {
        let amplitude = input.abs();
        let coeff = if amplitude > self.compressor_envelope {
            0.08
        } else {
            0.012
        };
        self.compressor_envelope += coeff * (amplitude - self.compressor_envelope);

        let target = 0.24;
        let gain = if self.compressor_envelope > target {
            (target + (self.compressor_envelope - target) * 0.38) / self.compressor_envelope
        } else {
            1.0 + privacy * 0.08
        };
        let amount = privacy * 0.65;
        let applied_gain = 1.0 * (1.0 - amount) + gain * amount;

        input * applied_gain
    }

    fn flatten_personality(&mut self, input: f32, amount: f32) -> f32 {
        let amount = amount.clamp(0.0, 0.72);
        self.prosody_state += 0.0045 * (input - self.prosody_state);

        let flattened = input - self.prosody_state * amount;
        let clarity = flattened + (input - self.speech_state) * (0.06 + amount * 0.04);

        clarity * (1.0 + amount * 0.08)
    }
}

fn raised_cosine(phase: f32) -> f32 {
    0.5 - 0.5 * (TWO_PI * phase).cos()
}

fn advance_pitch_phase(ratio: f32, phase: f32) -> f32 {
    let step = (ratio - 1.0).abs().max(0.08) / PITCH_WINDOW as f32;
    (phase + step).fract()
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn effective_privacy(value: f32) -> f32 {
    value.clamp(0.0, 1.0).powf(0.78)
}

fn soft_clip(sample: f32) -> f32 {
    sample / (1.0 + sample.abs() * 0.35)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_attenuates_sustained_quiet_noise() {
        let params = SharedDspParams::new();
        params.set_noise_gate(0.1);

        let mut processor = VoiceProcessor::default();
        let mut output = 0.0;
        for _ in 0..4000 {
            output = processor.process_sample(0.02, &params);
        }

        assert!(output.abs() < 0.02);
        assert!(output.abs() > 0.0);
    }

    #[test]
    fn gain_is_clamped() {
        let params = SharedDspParams::new();
        params.set_gain(10.0);
        params.set_robot_amount(0.0);

        let mut processor = VoiceProcessor::default();

        assert_eq!(processor.process_sample(0.5, &params), 1.0);
    }

    #[test]
    fn privacy_chain_changes_sustained_voice() {
        let params = SharedDspParams::new();
        params.set_robot_amount(1.0);

        let mut processor = VoiceProcessor::default();
        let mut output = 0.0;
        for _ in 0..PITCH_WINDOW {
            output = processor.process_sample(0.3, &params);
        }

        assert_ne!(output, 0.3);
    }

    #[test]
    fn default_privacy_changes_more_than_plain_gain() {
        let params = SharedDspParams::new();

        let mut processor = VoiceProcessor::default();
        let mut output = 0.0;
        for n in 0..PITCH_WINDOW {
            let sample = (TWO_PI * 140.0 * n as f32 / DEFAULT_SAMPLE_RATE as f32).sin() * 0.22;
            output = processor.process_sample(sample, &params);
        }

        assert!(output.abs() > 0.0);
        assert_ne!(output, 0.22);
    }

    #[test]
    fn monotone_keeps_signal_voiced_without_buzz_carrier() {
        let params = SharedDspParams::new();
        params.set_robot_amount(0.7);
        params.set_monotone(true);

        let mut processor = VoiceProcessor::default();
        let mut max = 0.0_f32;
        for n in 0..PITCH_WINDOW {
            let sample = (TWO_PI * 180.0 * n as f32 / DEFAULT_SAMPLE_RATE as f32).sin() * 0.25;
            max = max.max(processor.process_sample(sample, &params).abs());
        }

        assert!(max > 0.02);
        assert!(max < 1.0);
    }

    #[test]
    fn delay_reader_wraps_exact_buffer_boundary() {
        let mut processor = VoiceProcessor::default();
        processor.pitch_buffer[0] = 0.25;
        processor.pitch_buffer[1] = 0.5;

        assert_eq!(processor.read_delay(PITCH_BUFFER as f32), 0.25);
    }
}
