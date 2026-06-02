use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use atomic_float::AtomicF32;

pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;
const TWO_PI: f32 = std::f32::consts::PI * 2.0;

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
            robot_amount: AtomicF32::new(0.35),
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
    phase: f32,
}

impl Default for VoiceProcessor {
    fn default() -> Self {
        Self { phase: 0.0 }
    }
}

impl VoiceProcessor {
    pub fn process_sample(&mut self, input: f32, params: &SharedDspParams) -> f32 {
        let params = params.snapshot();
        let mut sample = if input.abs() < params.noise_gate {
            0.0
        } else {
            input * params.gain
        };

        if params.robot_amount > 0.0 {
            self.phase += TWO_PI * 70.0 / DEFAULT_SAMPLE_RATE as f32;
            if self.phase >= TWO_PI {
                self.phase -= TWO_PI;
            }
            let carrier = self.phase.sin();
            sample = sample * (1.0 - params.robot_amount) + sample * carrier * params.robot_amount;
        }

        if params.monotone {
            sample = sample.signum() * sample.abs().sqrt() * 0.45;
        }

        sample.clamp(-1.0, 1.0)
    }
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

        let mut processor = VoiceProcessor::default();

        assert_eq!(processor.process_sample(0.5, &params), 1.0);
    }
}
