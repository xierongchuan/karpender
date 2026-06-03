mod math;
mod noise;
mod params;
mod processor;
mod recipe;

#[cfg(test)]
mod tests;

pub use params::SharedDspParams;
pub use processor::VoiceProcessor;

pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;
pub(crate) const TWO_PI: f32 = std::f32::consts::PI * 2.0;
pub(crate) const PITCH_WINDOW: usize = 1536;
pub(crate) const PITCH_BUFFER: usize = PITCH_WINDOW * 4;
