use super::{MAX_GRAIN, MIN_GRAIN, PITCH_BUFFER, TWO_PI};

/// Extra delay in front of the moving tap so interpolation never reads samples
/// that have not been written yet.
const BASE_DELAY: f32 = 24.0;

/// How fast the grain length is allowed to follow the tracked pitch. Resizing a
/// grain moves both taps, so it has to be slow enough to stay inaudible.
const GRAIN_GLIDE: f32 = 0.0006;

/// Grain length in samples for a detected pitch period, or [`MIN_GRAIN`] when
/// the input is unvoiced and there is no period to lock to.
///
/// The grain has to hold a whole number of input periods, otherwise the wrap of
/// the delay ramp lands mid cycle and the shifter puts out a comb of sidebands
/// around the wrong frequency instead of the requested pitch. Two periods
/// rather than one, because the second tap sits half a grain behind the first
/// and would be in antiphase with an odd count.
///
/// See `experiments/pitch_shifter_spectrum.py` for the measurement this comes
/// from.
pub(super) fn grain_window(period: f32) -> f32 {
    if !period.is_finite() || period <= 0.0 {
        return MIN_GRAIN;
    }

    (2.0 * period).clamp(MIN_GRAIN, MAX_GRAIN)
}

/// Granular (delay line) pitch shifter with two taps crossfaded by a raised
/// cosine window. The taps are half a grain apart, so the tap that wraps is
/// always the one faded out.
#[derive(Debug)]
pub(super) struct PitchShifter {
    buffer: Vec<f32>,
    write_pos: usize,
    phase: f32,
    grain: f32,
}

impl Default for PitchShifter {
    fn default() -> Self {
        Self {
            buffer: vec![0.0; PITCH_BUFFER],
            write_pos: 0,
            phase: 0.0,
            grain: MIN_GRAIN,
        }
    }
}

impl PitchShifter {
    pub(super) fn process(&mut self, input: f32, ratio: f32, grain: f32) -> f32 {
        self.grain += GRAIN_GLIDE * (grain.clamp(MIN_GRAIN, MAX_GRAIN) - self.grain);

        self.buffer[self.write_pos] = input;
        self.write_pos = (self.write_pos + 1) % self.buffer.len();

        let first = self.read_phase(self.phase);
        let second = self.read_phase((self.phase + 0.5).fract());
        let fade = crossfade(self.phase);
        self.phase = advance_phase(self.phase, ratio, self.grain);

        first * fade + second * (1.0 - fade)
    }

    fn read_phase(&self, phase: f32) -> f32 {
        self.read_delay(phase * self.grain + BASE_DELAY)
    }

    pub(super) fn read_delay(&self, delay: f32) -> f32 {
        let len = self.buffer.len();
        let read = (self.write_pos as f32 - delay).rem_euclid(len as f32);
        let floor = read.floor();
        let i0 = (floor as usize) % len;
        let i1 = (i0 + 1) % len;
        let frac = read - floor;

        self.buffer[i0] * (1.0 - frac) + self.buffer[i1] * frac
    }

    /// Longest delay the taps can currently reach, which is what the chain
    /// contributes to end to end latency.
    #[cfg(test)]
    pub(super) fn delay_samples(&self) -> f32 {
        self.grain + BASE_DELAY
    }

    #[cfg(test)]
    pub(super) fn buffer_mut(&mut self) -> &mut [f32] {
        &mut self.buffer
    }

    #[cfg(test)]
    pub(super) fn set_write_pos(&mut self, position: usize) {
        self.write_pos = position;
    }

    /// Skips the glide so a test can start from a settled grain.
    #[cfg(test)]
    pub(super) fn set_grain(&mut self, grain: f32) {
        self.grain = grain.clamp(MIN_GRAIN, MAX_GRAIN);
    }
}

/// The read pointer has to advance at `ratio` samples per output sample, so the
/// delay changes by `1 - ratio` per sample. A ratio of exactly 1.0 therefore
/// freezes the delay and leaves the signal untouched.
pub(super) fn advance_phase(phase: f32, ratio: f32, grain: f32) -> f32 {
    let step = (1.0 - ratio) / grain.max(1.0);

    (phase + step).rem_euclid(1.0)
}

/// Weight of the first tap. Zero where that tap wraps, one where the other one
/// does, so neither discontinuity is audible.
pub(super) fn crossfade(phase: f32) -> f32 {
    0.5 * (1.0 - (TWO_PI * phase).cos())
}

/// Worst case delay of the shifter in samples, used by the latency test.
#[cfg(test)]
pub(super) fn max_delay_samples() -> f32 {
    MAX_GRAIN + BASE_DELAY
}
