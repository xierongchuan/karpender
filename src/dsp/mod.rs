use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use atomic_float::AtomicF32;

pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;
const TWO_PI: f32 = std::f32::consts::PI * 2.0;
const PITCH_WINDOW: usize = 1536;
const PITCH_BUFFER: usize = PITCH_WINDOW * 4;

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
            robot_amount: AtomicF32::new(0.65),
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
    low_state: f32,
    envelope: f32,
    monotone_phase: f32,
}

impl Default for VoiceProcessor {
    fn default() -> Self {
        Self {
            pitch_buffer: vec![0.0; PITCH_BUFFER],
            write_pos: 0,
            down_phase: 0.0,
            up_phase: 0.5,
            low_state: 0.0,
            envelope: 0.0,
            monotone_phase: 0.0,
        }
    }
}

impl VoiceProcessor {
    pub fn process_sample(&mut self, input: f32, params: &SharedDspParams) -> f32 {
        let params = params.snapshot();
        let gated = if input.abs() < params.noise_gate {
            0.0
        } else {
            input * params.gain
        };

        let privacy = params.robot_amount.clamp(0.0, 1.0);
        self.pitch_buffer[self.write_pos] = gated;
        let shifted_down = self.pitch_tap_pair(0.72, self.down_phase);
        let shifted_up = self.pitch_tap_pair(1.18, self.up_phase);
        self.down_phase = advance_pitch_phase(0.72, self.down_phase);
        self.up_phase = advance_pitch_phase(1.18, self.up_phase);
        self.write_pos = (self.write_pos + 1) % self.pitch_buffer.len();

        let shifted = shifted_down * 0.72 + shifted_up * 0.28;
        let reshaped = self.formant_mask(shifted, privacy);
        let flattened = soft_limit(reshaped * (1.0 + privacy * 0.8));

        let mut sample = gated * (1.0 - privacy) + flattened * privacy;

        if params.monotone {
            sample = self.monotone_vocoder(sample, privacy);
        }

        sample.clamp(-1.0, 1.0)
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

    fn formant_mask(&mut self, input: f32, privacy: f32) -> f32 {
        let low_coeff = 0.075 + privacy * 0.045;
        self.low_state += low_coeff * (input - self.low_state);
        let high = input - self.low_state;

        let darker_voice = self.low_state * 0.85 - high * 0.35;
        let nasal_voice = self.low_state * 0.35 + high * 1.25;
        let blend = privacy * privacy;

        darker_voice * (1.0 - blend) + nasal_voice * blend
    }

    fn monotone_vocoder(&mut self, input: f32, privacy: f32) -> f32 {
        let attack = 0.18;
        let release = 0.025;
        let amplitude = input.abs();
        let coeff = if amplitude > self.envelope {
            attack
        } else {
            release
        };
        self.envelope += coeff * (amplitude - self.envelope);

        self.monotone_phase += TWO_PI * 118.0 / DEFAULT_SAMPLE_RATE as f32;
        if self.monotone_phase >= TWO_PI {
            self.monotone_phase -= TWO_PI;
        }

        let carrier =
            self.monotone_phase.sin().signum() * 0.72 + (self.monotone_phase * 2.0).sin() * 0.18;
        let synthetic = carrier * self.envelope.sqrt() * 0.75;
        input * (1.0 - privacy) + synthetic * privacy
    }
}

fn raised_cosine(phase: f32) -> f32 {
    0.5 - 0.5 * (TWO_PI * phase).cos()
}

fn advance_pitch_phase(ratio: f32, phase: f32) -> f32 {
    let step = (ratio - 1.0).abs().max(0.08) / PITCH_WINDOW as f32;
    (phase + step).fract()
}

fn soft_limit(sample: f32) -> f32 {
    sample / (1.0 + sample.abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_mutes_quiet_samples() {
        let params = SharedDspParams::new();
        params.set_noise_gate(0.1);

        let mut processor = VoiceProcessor::default();

        assert_eq!(processor.process_sample(0.05, &params), 0.0);
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
    fn delay_reader_wraps_exact_buffer_boundary() {
        let mut processor = VoiceProcessor::default();
        processor.pitch_buffer[0] = 0.25;
        processor.pitch_buffer[1] = 0.5;

        assert_eq!(processor.read_delay(PITCH_BUFFER as f32), 0.25);
    }
}
