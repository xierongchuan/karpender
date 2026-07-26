use super::{DEFAULT_SAMPLE_RATE, biquad::Biquad};

/// The tracker runs on a decimated copy of the signal: speech fundamentals live
/// below 400 Hz, so 12 kHz is plenty and the correlation gets four times cheaper.
const DECIMATION: usize = 4;
const TRACKER_RATE: f32 = DEFAULT_SAMPLE_RATE as f32 / DECIMATION as f32;
const ANALYSIS_CUTOFF_HZ: f32 = 1_100.0;

const WINDOW: usize = 384;
/// 12000 / 172 ~= 70 Hz, 12000 / 38 ~= 316 Hz. Covers low male to high female.
const MIN_LAG: usize = 38;
const MAX_LAG: usize = 172;
const LAG_COUNT: usize = MAX_LAG - MIN_LAG + 1;
const FRAME: usize = WINDOW + MAX_LAG + 1;
const RING: usize = FRAME * 2;

const VOICED_SCORE: f32 = 0.34;
const OCTAVE_TOLERANCE: f32 = 0.86;
const SILENCE_ENERGY: f32 = WINDOW as f32 * 1.0e-7;

/// Autocorrelation pitch tracker. One correlation lag is evaluated per decimated
/// sample, which spreads the cost evenly instead of stalling a single callback.
#[derive(Debug)]
pub(super) struct PitchTracker {
    lowpass: Biquad,
    decimation_counter: usize,
    ring: Vec<f32>,
    write_pos: usize,
    frame: Vec<f32>,
    scores: Vec<f32>,
    lag: usize,
    head_energy: f32,
    tail_energy: f32,
    running: bool,
    frequency: f32,
    confidence: f32,
}

impl Default for PitchTracker {
    fn default() -> Self {
        Self {
            lowpass: Biquad::lowpass(ANALYSIS_CUTOFF_HZ, 0.707),
            decimation_counter: 0,
            ring: vec![0.0; RING],
            write_pos: 0,
            frame: vec![0.0; FRAME],
            scores: vec![0.0; LAG_COUNT],
            lag: MIN_LAG,
            head_energy: 0.0,
            tail_energy: 0.0,
            running: false,
            frequency: 0.0,
            confidence: 0.0,
        }
    }
}

impl PitchTracker {
    pub(super) fn observe(&mut self, input: f32) {
        let filtered = self.lowpass.process(input);
        self.decimation_counter += 1;
        if self.decimation_counter < DECIMATION {
            return;
        }
        self.decimation_counter = 0;

        self.ring[self.write_pos] = filtered;
        self.write_pos = (self.write_pos + 1) % RING;

        if self.running {
            self.step_lag();
        } else {
            self.start_cycle();
        }
    }

    /// Estimated fundamental in Hz, or zero while the input is unvoiced.
    pub(super) fn frequency(&self) -> f32 {
        self.frequency
    }

    pub(super) fn confidence(&self) -> f32 {
        self.confidence
    }

    /// Copies the newest frame so the correlation works on a stable snapshot
    /// while new audio keeps arriving.
    fn start_cycle(&mut self) {
        let start = (self.write_pos + RING - FRAME) % RING;
        for index in 0..FRAME {
            self.frame[index] = self.ring[(start + index) % RING];
        }

        self.head_energy = self.frame[..WINDOW].iter().map(|value| value * value).sum();
        self.tail_energy = self.frame[MIN_LAG..MIN_LAG + WINDOW]
            .iter()
            .map(|value| value * value)
            .sum();
        self.lag = MIN_LAG;
        self.running = true;
    }

    fn step_lag(&mut self) {
        let lag = self.lag;
        let mut correlation = 0.0;
        for index in 0..WINDOW {
            correlation += self.frame[index] * self.frame[index + lag];
        }

        let denominator = (self.head_energy * self.tail_energy).max(1.0e-12).sqrt();
        self.scores[lag - MIN_LAG] = correlation / denominator;

        let leaving = self.frame[lag];
        let entering = self.frame[lag + WINDOW];
        self.tail_energy = (self.tail_energy - leaving * leaving + entering * entering).max(0.0);
        self.lag += 1;

        if self.lag > MAX_LAG {
            self.finish_cycle();
            self.running = false;
        }
    }

    fn finish_cycle(&mut self) {
        let best = self
            .scores
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);

        if self.head_energy < SILENCE_ENERGY || best < VOICED_SCORE {
            self.frequency = 0.0;
            self.confidence = 0.0;
            return;
        }

        // Sub harmonics correlate almost as well as the true period, so pick the
        // shortest lag that is close to the best score instead of the best one.
        let threshold = best * OCTAVE_TOLERANCE;
        let index = self
            .scores
            .iter()
            .position(|score| *score >= threshold)
            .unwrap_or(0);
        let lag = (MIN_LAG + index) as f32 + self.interpolate(index);

        self.frequency = TRACKER_RATE / lag.max(1.0);
        self.confidence = best.clamp(0.0, 1.0);
    }

    /// Parabolic refinement around the chosen correlation peak.
    fn interpolate(&self, index: usize) -> f32 {
        if index == 0 || index + 1 >= LAG_COUNT {
            return 0.0;
        }

        let left = self.scores[index - 1];
        let center = self.scores[index];
        let right = self.scores[index + 1];
        let denominator = left - 2.0 * center + right;
        if denominator.abs() < 1.0e-9 {
            return 0.0;
        }

        (0.5 * (left - right) / denominator).clamp(-0.5, 0.5)
    }
}
