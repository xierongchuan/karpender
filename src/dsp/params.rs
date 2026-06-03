use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

use atomic_float::AtomicF32;

use crate::config::VoiceMode;

#[derive(Debug, Default)]
pub struct SharedDspParams {
    gain: AtomicF32,
    noise_gate: AtomicF32,
    robot_amount: AtomicF32,
    monotone: AtomicBool,
    voice_mode: AtomicU8,
}

impl SharedDspParams {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            gain: AtomicF32::new(1.0),
            noise_gate: AtomicF32::new(0.03),
            robot_amount: AtomicF32::new(0.55),
            monotone: AtomicBool::new(false),
            voice_mode: AtomicU8::new(voice_mode_to_u8(VoiceMode::Masked)),
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

    pub fn set_voice_mode(&self, mode: VoiceMode) {
        self.voice_mode
            .store(voice_mode_to_u8(mode), Ordering::Relaxed);
    }

    pub(super) fn snapshot(&self) -> DspSnapshot {
        DspSnapshot {
            gain: self.gain.load(Ordering::Relaxed),
            noise_gate: self.noise_gate.load(Ordering::Relaxed),
            robot_amount: self.robot_amount.load(Ordering::Relaxed),
            monotone: self.monotone.load(Ordering::Relaxed),
            voice_mode: voice_mode_from_u8(self.voice_mode.load(Ordering::Relaxed)),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct DspSnapshot {
    pub(super) gain: f32,
    pub(super) noise_gate: f32,
    pub(super) robot_amount: f32,
    pub(super) monotone: bool,
    pub(super) voice_mode: VoiceMode,
}

fn voice_mode_to_u8(mode: VoiceMode) -> u8 {
    match mode {
        VoiceMode::Masked => 0,
        VoiceMode::BrightStranger => 1,
        VoiceMode::DeepMorph => 2,
        VoiceMode::CinematicHigh => 3,
    }
}

fn voice_mode_from_u8(value: u8) -> VoiceMode {
    match value {
        1 => VoiceMode::BrightStranger,
        2 => VoiceMode::DeepMorph,
        3 => VoiceMode::CinematicHigh,
        _ => VoiceMode::Masked,
    }
}
