use super::{
    DEFAULT_SAMPLE_RATE,
    formant::BAND_COUNT,
    noise::{initial_jitter_seed, xorshift32},
};

/// Static per session spread. Two runs of the same preset land on two different
/// voices, so recordings cannot be linked back to a single synthetic identity.
const PITCH_SPREAD_SEMITONES: f32 = 2.2;
const FORMANT_SPREAD: f32 = 0.075;
const TILT_SPREAD_DB: f32 = 2.4;
const COLOR_SPREAD_DB: f32 = 2.2;

/// Slow drift on top of the static offsets. It keeps long recordings from
/// settling on a stable set of speaker features.
const PITCH_DRIFT_SEMITONES: f32 = 0.35;
const FORMANT_DRIFT: f32 = 0.022;
const DRIFT_SECONDS: f32 = 5.5;

/// A random walk that eases toward a fresh target every few seconds.
#[derive(Debug, Clone, Copy)]
struct Drift {
    value: f32,
    target: f32,
    range: f32,
    seed: u32,
    countdown: u32,
}

impl Drift {
    fn new(seed: u32, range: f32) -> Self {
        Self {
            value: 0.0,
            target: 0.0,
            range,
            seed,
            countdown: 0,
        }
    }

    fn advance(&mut self, control_interval: usize) {
        if self.countdown == 0 {
            self.seed = xorshift32(self.seed);
            self.target = unit_signed(self.seed) * self.range;
            let ticks = DRIFT_SECONDS * DEFAULT_SAMPLE_RATE as f32 / control_interval as f32;
            self.countdown = ticks.max(1.0) as u32;
        } else {
            self.countdown -= 1;
        }

        // One pole glide, so the walk never steps audibly.
        self.value += 0.004 * (self.target - self.value);
    }
}

/// Randomised voice identity applied on top of the mode recipe.
#[derive(Debug)]
pub(super) struct SessionIdentity {
    pitch_semitones: f32,
    formant_offset: f32,
    tilt_db: f32,
    color_db: [f32; BAND_COUNT],
    pitch_drift: Drift,
    formant_drift: Drift,
}

impl Default for SessionIdentity {
    fn default() -> Self {
        Self::from_seed(initial_jitter_seed())
    }
}

impl SessionIdentity {
    pub(super) fn from_seed(seed: u32) -> Self {
        let mut seed = seed.max(1);
        let mut next = || {
            seed = xorshift32(seed);
            unit_signed(seed)
        };

        let pitch_semitones = next() * PITCH_SPREAD_SEMITONES;
        let formant_offset = next() * FORMANT_SPREAD;
        let tilt_db = next() * TILT_SPREAD_DB;
        let mut color_db = [0.0_f32; BAND_COUNT];
        for value in color_db.iter_mut() {
            *value = next() * COLOR_SPREAD_DB;
        }

        let pitch_seed = xorshift32(seed ^ 0x9e37_79b9);
        let formant_seed = xorshift32(pitch_seed ^ 0x85eb_ca6b);

        Self {
            pitch_semitones,
            formant_offset,
            tilt_db,
            color_db,
            pitch_drift: Drift::new(pitch_seed, PITCH_DRIFT_SEMITONES),
            formant_drift: Drift::new(formant_seed, FORMANT_DRIFT),
        }
    }

    pub(super) fn advance(&mut self, control_interval: usize) {
        self.pitch_drift.advance(control_interval);
        self.formant_drift.advance(control_interval);
    }

    pub(super) fn pitch_semitones(&self, privacy: f32) -> f32 {
        (self.pitch_semitones + self.pitch_drift.value) * privacy
    }

    /// Multiplicative formant offset, centred on 1.0.
    pub(super) fn formant_ratio(&self, privacy: f32) -> f32 {
        1.0 + (self.formant_offset + self.formant_drift.value) * privacy
    }

    pub(super) fn tilt_db(&self, privacy: f32) -> f32 {
        self.tilt_db * privacy
    }

    pub(super) fn color_db(&self, privacy: f32, into: &mut [f32; BAND_COUNT]) {
        for (slot, value) in into.iter_mut().zip(self.color_db.iter()) {
            *slot = value * privacy;
        }
    }
}

fn unit_signed(seed: u32) -> f32 {
    (seed as f32 / u32::MAX as f32) * 2.0 - 1.0
}
