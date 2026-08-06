use super::{
    biquad::{Biquad, band_parts},
    math::amplitude_to_db,
};

/// Log spaced analysis bands covering the speech range: 160 Hz to about 4.7 kHz.
pub(super) const BAND_COUNT: usize = 8;
const FIRST_CENTER_HZ: f32 = 160.0;
const BAND_RATIO: f32 = 1.62;
const ANALYSIS_Q: f32 = 1.7;
const SHAPE_Q: f32 = 1.2;

const ATTACK: f32 = 0.020;
const RELEASE: f32 = 0.0035;
/// Bands this far below the loudest one carry no usable envelope information.
const ENVELOPE_FLOOR_DB: f32 = 42.0;
const MAX_BAND_DB: f32 = 10.0;
/// Largest coefficient change per control tick, keeps filter updates click free.
const SLEW_DB: f32 = 0.35;

pub(super) fn band_center(index: usize) -> f32 {
    FIRST_CENTER_HZ * BAND_RATIO.powi(index as i32)
}

/// Spectral envelope remapper. It measures the envelope of its own input with a
/// bandpass bank and rebuilds it at a shifted position with a peaking cascade,
/// which moves the formants without touching the fundamental.
#[derive(Debug)]
pub(super) struct FormantShaper {
    analysis: [Biquad; BAND_COUNT],
    shape: [Biquad; BAND_COUNT],
    parts: [(f32, f32); BAND_COUNT],
    envelope: [f32; BAND_COUNT],
    applied_db: [f32; BAND_COUNT],
}

impl Default for FormantShaper {
    fn default() -> Self {
        let mut analysis = [Biquad::default(); BAND_COUNT];
        let mut parts = [(0.0, 0.0); BAND_COUNT];
        for index in 0..BAND_COUNT {
            let center = band_center(index);
            analysis[index] = Biquad::bandpass(center, ANALYSIS_Q);
            parts[index] = band_parts(center, SHAPE_Q);
        }

        Self {
            analysis,
            shape: [Biquad::default(); BAND_COUNT],
            parts,
            envelope: [0.0; BAND_COUNT],
            applied_db: [0.0; BAND_COUNT],
        }
    }
}

impl FormantShaper {
    /// Tracks the per band level of the signal that is about to be shaped.
    pub(super) fn observe(&mut self, input: f32) {
        for index in 0..BAND_COUNT {
            let magnitude = self.analysis[index].process(input).abs();
            let coefficient = if magnitude > self.envelope[index] {
                ATTACK
            } else {
                RELEASE
            };
            self.envelope[index] += coefficient * (magnitude - self.envelope[index]);
        }
    }

    /// Recomputes the peaking cascade. `shift` is the factor the envelope should
    /// move by, `strength` how much of that correction to apply, and `color_db`
    /// the per band offset that carries the session identity.
    pub(super) fn update(
        &mut self,
        shift: f32,
        strength: f32,
        tilt_db: f32,
        color_db: &[f32; BAND_COUNT],
    ) {
        let mut envelope_db = [0.0_f32; BAND_COUNT];
        for (slot, amplitude) in envelope_db.iter_mut().zip(self.envelope.iter()) {
            *slot = amplitude_to_db(*amplitude);
        }

        let peak = envelope_db
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        let floor = peak - ENVELOPE_FLOOR_DB;
        for value in envelope_db.iter_mut() {
            *value = value.max(floor);
        }

        // A shift of `BAND_RATIO` is exactly one band, so the source position of
        // band k is k - log(shift) / log(BAND_RATIO).
        let offset = shift.max(0.05).ln() / BAND_RATIO.ln();
        let mut remap = [0.0_f32; BAND_COUNT];
        for index in 0..BAND_COUNT {
            let source = sample_envelope(&envelope_db, index as f32 - offset);
            remap[index] = (source - envelope_db[index]) * strength;
        }

        // Moving the envelope must not move the loudness with it.
        let mean = remap.iter().sum::<f32>() / BAND_COUNT as f32;
        let last = (BAND_COUNT - 1) as f32;

        for index in 0..BAND_COUNT {
            let tilt = tilt_db * (index as f32 / last - 0.5) * 2.0;
            let target =
                (remap[index] - mean + tilt + color_db[index]).clamp(-MAX_BAND_DB, MAX_BAND_DB);
            let delta = (target - self.applied_db[index]).clamp(-SLEW_DB, SLEW_DB);
            self.applied_db[index] += delta;

            let (cos_w0, alpha) = self.parts[index];
            self.shape[index].set_peaking(cos_w0, alpha, self.applied_db[index]);
        }
    }

    #[cfg(test)]
    pub(super) fn gain_db(&self, band: usize) -> f32 {
        self.applied_db[band]
    }

    pub(super) fn process(&mut self, input: f32) -> f32 {
        let mut sample = input;
        for filter in self.shape.iter_mut() {
            sample = filter.process(sample);
        }

        sample
    }
}

/// Linear interpolation of the band envelope at a fractional band position.
fn sample_envelope(envelope_db: &[f32; BAND_COUNT], position: f32) -> f32 {
    let last = (BAND_COUNT - 1) as f32;
    let clamped = position.clamp(0.0, last);
    let low = clamped.floor();
    let index = low as usize;
    if index >= BAND_COUNT - 1 {
        return envelope_db[BAND_COUNT - 1];
    }

    let fraction = clamped - low;
    envelope_db[index] * (1.0 - fraction) + envelope_db[index + 1] * fraction
}
