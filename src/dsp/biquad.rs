use super::{DEFAULT_SAMPLE_RATE, TWO_PI};

/// Direct form 1 biquad. Coefficients are already normalized by `a0`.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    pub(super) fn bandpass(center_hz: f32, q: f32) -> Self {
        let mut filter = Self::default();
        let (cos_w0, alpha) = band_parts(center_hz, q);
        let a0 = 1.0 + alpha;

        filter.b0 = alpha / a0;
        filter.b1 = 0.0;
        filter.b2 = -alpha / a0;
        filter.a1 = -2.0 * cos_w0 / a0;
        filter.a2 = (1.0 - alpha) / a0;
        filter
    }

    /// Updates a peaking filter in place. `cos_w0` and `alpha` stay constant for
    /// a fixed center frequency, so gain changes cost no trigonometry.
    pub(super) fn set_peaking(&mut self, cos_w0: f32, alpha: f32, gain_db: f32) {
        let amplitude = (10.0_f32).powf(gain_db / 40.0);
        let a0 = 1.0 + alpha / amplitude;

        self.b0 = (1.0 + alpha * amplitude) / a0;
        self.b1 = -2.0 * cos_w0 / a0;
        self.b2 = (1.0 - alpha * amplitude) / a0;
        self.a1 = -2.0 * cos_w0 / a0;
        self.a2 = (1.0 - alpha / amplitude) / a0;
    }

    /// Second order low pass used to protect the pitch tracker from aliasing.
    pub(super) fn lowpass(cutoff_hz: f32, q: f32) -> Self {
        let mut filter = Self::default();
        let (cos_w0, alpha) = band_parts(cutoff_hz, q);
        let a0 = 1.0 + alpha;
        let base = (1.0 - cos_w0) / 2.0;

        filter.b0 = base / a0;
        filter.b1 = (1.0 - cos_w0) / a0;
        filter.b2 = base / a0;
        filter.a1 = -2.0 * cos_w0 / a0;
        filter.a2 = (1.0 - alpha) / a0;
        filter
    }

    pub(super) fn process(&mut self, input: f32) -> f32 {
        let output = self.b0 * input + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        let output = if output.is_finite() { output } else { 0.0 };

        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;

        output
    }
}

/// Shared RBJ cookbook intermediates for a center frequency and Q.
pub(super) fn band_parts(center_hz: f32, q: f32) -> (f32, f32) {
    let nyquist = DEFAULT_SAMPLE_RATE as f32 * 0.5;
    let center = center_hz.clamp(20.0, nyquist * 0.94);
    let w0 = TWO_PI * center / DEFAULT_SAMPLE_RATE as f32;
    let alpha = w0.sin() / (2.0 * q.max(0.1));

    (w0.cos(), alpha)
}

/// One pole high pass, used as a DC blocker on the microphone input.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct DcBlocker {
    previous_input: f32,
    previous_output: f32,
}

impl DcBlocker {
    pub(super) fn process(&mut self, input: f32) -> f32 {
        const POLE: f32 = 0.9975; // ~ 19 Hz corner at 48 kHz

        let output = input - self.previous_input + POLE * self.previous_output;
        self.previous_input = input;
        self.previous_output = if output.is_finite() { output } else { 0.0 };

        self.previous_output
    }
}
