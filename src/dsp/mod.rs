mod biquad;
mod formant;
mod identity;
mod math;
mod noise;
mod params;
mod pitch;
mod processor;
mod recipe;
mod tracker;

#[cfg(test)]
mod tests;

pub use params::SharedDspParams;
pub use processor::VoiceProcessor;

pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;
pub(crate) const TWO_PI: f32 = std::f32::consts::PI * 2.0;
/// Grain size limits of the delay line pitch shifter. The grain tracks two
/// periods of the detected pitch, so these only bound it: 256 samples is 5.3 ms
/// and covers unvoiced input, 1152 samples is 24 ms and reaches down to an 83 Hz
/// voice. The grain is the dominant part of the algorithmic delay, so a typical
/// 150 Hz voice pays 640 samples, or 13 ms, not the maximum.
pub(crate) const MIN_GRAIN: f32 = 256.0;
pub(crate) const MAX_GRAIN: f32 = 1152.0;
pub(crate) const PITCH_BUFFER: usize = 4096;
/// Control rate of the voice model, in samples. 64 samples is 1.3 ms, fast
/// enough to track a syllable and cheap enough for the realtime callback.
pub(crate) const CONTROL_INTERVAL: usize = 64;
