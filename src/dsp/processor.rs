use super::{
    CONTROL_INTERVAL, DEFAULT_SAMPLE_RATE, MIN_GRAIN,
    biquad::DcBlocker,
    formant::{BAND_COUNT, FormantShaper},
    identity::SessionIdentity,
    math::{effective_privacy, input_drive, one_pole_coeff, ratio_blend, smoothstep, soft_clip},
    noise::{initial_jitter_seed, xorshift32},
    params::SharedDspParams,
    pitch::{PitchShifter, grain_window},
    recipe::{VoiceRecipe, voice_recipe},
    tracker::PitchTracker,
};

const MIN_EXPANDER_GAIN: f32 = 0.48;
/// Pitch correction range, about minus seven to plus eight semitones. Wider than
/// this and the delay line shifter starts to sound like a cartoon.
const MIN_PITCH_RATIO: f32 = 0.66;
const MAX_PITCH_RATIO: f32 = 1.58;
/// The shaper can move formants by roughly plus or minus five semitones before
/// the eight band cascade runs out of resolution.
const MIN_SHAPE_RATIO: f32 = 0.72;
const MAX_SHAPE_RATIO: f32 = 1.40;
const ENVELOPE_REMAP_STRENGTH: f32 = 0.85;

#[derive(Debug)]
pub struct VoiceProcessor {
    dc_blocker: DcBlocker,
    tracker: PitchTracker,
    shifter: PitchShifter,
    shaper: FormantShaper,
    identity: SessionIdentity,
    color_db: [f32; BAND_COUNT],
    control_countdown: usize,
    pitch_ratio: f32,
    target_pitch_ratio: f32,
    shape_ratio: f32,
    target_shape_ratio: f32,
    target_grain: f32,
    ratio_coefficient: f32,
    breath_level: f32,
    target_breath_level: f32,
    expander_envelope: f32,
    expander_gain: f32,
    compressor_envelope: f32,
    output_envelope: f32,
    output_gain: f32,
    pub(super) noise_seed: u32,
    noise_state: f32,
}

impl Default for VoiceProcessor {
    fn default() -> Self {
        Self {
            dc_blocker: DcBlocker::default(),
            tracker: PitchTracker::default(),
            shifter: PitchShifter::default(),
            shaper: FormantShaper::default(),
            identity: SessionIdentity::default(),
            color_db: [0.0; BAND_COUNT],
            control_countdown: 0,
            pitch_ratio: 1.0,
            target_pitch_ratio: 1.0,
            shape_ratio: 1.0,
            target_shape_ratio: 1.0,
            target_grain: MIN_GRAIN,
            // About 45 ms to glide to a new ratio: fast enough to follow a
            // sentence, slow enough to stay free of zipper noise.
            ratio_coefficient: one_pole_coeff(0.045),
            breath_level: 0.0,
            target_breath_level: 0.0,
            expander_envelope: 0.0,
            expander_gain: 1.0,
            compressor_envelope: 0.0,
            output_envelope: 0.0,
            output_gain: 1.0,
            noise_seed: initial_jitter_seed() ^ 0x85eb_ca6b,
            noise_state: 0.0,
        }
    }
}

impl VoiceProcessor {
    pub fn process_sample(&mut self, input: f32, params: &SharedDspParams) -> f32 {
        let params = params.snapshot();
        let privacy = effective_privacy(params.robot_amount);
        let recipe = voice_recipe(params.voice_mode);

        let driven = self.dc_blocker.process(input * input_drive(params.gain));
        let cleaned = self.noise_cleanup(driven, params.noise_gate);

        self.tracker.observe(cleaned);

        if self.control_countdown == 0 {
            self.update_controls(&recipe, privacy, params.monotone);
            self.control_countdown = CONTROL_INTERVAL;
        }
        self.control_countdown -= 1;

        self.pitch_ratio += self.ratio_coefficient * (self.target_pitch_ratio - self.pitch_ratio);
        self.shape_ratio += self.ratio_coefficient * (self.target_shape_ratio - self.shape_ratio);
        self.breath_level += 0.0015 * (self.target_breath_level - self.breath_level);

        let shifted = self
            .shifter
            .process(cleaned, self.pitch_ratio, self.target_grain);
        self.shaper.observe(shifted);
        let shaped = self.shaper.process(shifted);
        let breathed = shaped + self.next_soft_noise() * self.breath_level;
        let compressed = self.speech_compress(breathed, privacy);
        let leveled = self.stabilize_loudness(compressed, recipe.output_level, params.monotone);

        soft_clip(leveled).clamp(-1.0, 1.0)
    }

    /// Everything that only needs to move at control rate. Running this once per
    /// 64 samples keeps the per sample path down to filters and delay reads.
    fn update_controls(&mut self, recipe: &VoiceRecipe, privacy: f32, monotone: bool) {
        self.identity.advance(CONTROL_INTERVAL);

        let normalize = if monotone {
            recipe.normalize.max(0.9)
        } else {
            recipe.normalize * (0.30 + 0.70 * privacy)
        }
        .clamp(0.0, 1.0);

        let target_f0 =
            recipe.target_f0 * semitone_ratio(self.identity.pitch_semitones(privacy)).max(0.1);
        let detected = self.tracker.frequency();
        if detected > 0.0 {
            let wanted = (target_f0 / detected).clamp(MIN_PITCH_RATIO, MAX_PITCH_RATIO);
            self.target_pitch_ratio = ratio_blend(1.0, wanted, normalize);
        }

        // The shifter needs the period of its own input, which is the detected
        // pitch, to keep its grain phase aligned.
        let period = if detected > 0.0 {
            DEFAULT_SAMPLE_RATE as f32 / detected
        } else {
            0.0
        };
        self.target_grain = grain_window(period);

        let wanted_formant = recipe.formant_ratio * self.identity.formant_ratio(privacy);
        self.target_shape_ratio =
            (wanted_formant / self.target_pitch_ratio).clamp(MIN_SHAPE_RATIO, MAX_SHAPE_RATIO);

        self.identity.color_db(privacy, &mut self.color_db);
        let tilt = recipe.tilt_db + self.identity.tilt_db(privacy);
        self.shaper.update(
            self.shape_ratio,
            ENVELOPE_REMAP_STRENGTH,
            tilt,
            &self.color_db,
        );

        // Breath only follows voiced speech, so silence stays silent.
        let voiced =
            smoothstep(self.tracker.confidence() * 1.6) * (self.output_envelope * 6.0).min(1.0);
        self.target_breath_level = recipe.breath * voiced * (0.4 + 0.6 * privacy);
    }

    pub(super) fn noise_cleanup(&mut self, input: f32, threshold: f32) -> f32 {
        let threshold = threshold.clamp(0.0, 0.4);
        let strength = threshold / 0.4;
        if strength <= 0.003 {
            self.expander_gain += 0.08 * (1.0 - self.expander_gain);
            return input * self.expander_gain;
        }

        let amplitude = input.abs();
        let coeff = if amplitude > self.expander_envelope {
            0.12
        } else {
            0.006
        };
        self.expander_envelope += coeff * (amplitude - self.expander_envelope);

        let floor = threshold * 0.16;
        let open = threshold * 0.70 + 0.018;
        let ratio = ((self.expander_envelope - floor) / (open - floor)).clamp(0.0, 1.0);
        let min_gain = (0.80 - strength * 0.32).max(MIN_EXPANDER_GAIN);
        let target_gain = min_gain + (1.0 - min_gain) * smoothstep(ratio);
        let gain_coeff = if target_gain > self.expander_gain {
            0.12
        } else {
            0.028
        };
        self.expander_gain += gain_coeff * (target_gain - self.expander_gain);

        input * self.expander_gain
    }

    fn speech_compress(&mut self, input: f32, privacy: f32) -> f32 {
        let amplitude = input.abs();
        let coeff = if amplitude > self.compressor_envelope {
            0.08
        } else {
            0.012
        };
        self.compressor_envelope += coeff * (amplitude - self.compressor_envelope);

        let target = 0.24;
        let gain = if self.compressor_envelope > target {
            (target + (self.compressor_envelope - target) * 0.38) / self.compressor_envelope
        } else {
            1.0
        };
        let amount = privacy * 0.65;

        input * (1.0 - amount + gain * amount)
    }

    /// Levels the output and, in monotone mode, also flattens the loudness
    /// contour, which is one of the strongest speaker cues after pitch.
    fn stabilize_loudness(&mut self, input: f32, target: f32, monotone: bool) -> f32 {
        let amplitude = input.abs();
        let coeff = if amplitude > self.output_envelope {
            0.035
        } else {
            0.004
        };
        self.output_envelope += coeff * (amplitude - self.output_envelope);

        let desired = if self.output_envelope > 0.018 {
            (target / self.output_envelope).clamp(0.45, 1.75)
        } else {
            1.0
        };
        let (down, up) = if monotone {
            (0.045, 0.012)
        } else {
            (0.018, 0.003)
        };
        let gain_coeff = if desired < self.output_gain { down } else { up };
        self.output_gain += gain_coeff * (desired - self.output_gain);

        input * self.output_gain
    }

    pub(super) fn next_soft_noise(&mut self) -> f32 {
        self.noise_seed = xorshift32(self.noise_seed);
        let white = (self.noise_seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
        self.noise_state += 0.12 * (white - self.noise_state);

        self.noise_state
    }

    /// Deterministic instance for tests: the session identity is otherwise
    /// seeded from the clock on purpose.
    #[cfg(test)]
    pub(super) fn seeded(seed: u32) -> Self {
        Self {
            identity: SessionIdentity::from_seed(seed),
            noise_seed: seed ^ 0x8765_4321,
            ..Self::default()
        }
    }

    #[cfg(test)]
    pub(super) fn pitch_ratio(&self) -> f32 {
        self.pitch_ratio
    }

    /// Delay of this instance right now, in samples. The grain follows the
    /// tracked pitch, so a high voice costs noticeably less than a low one.
    #[cfg(test)]
    pub(super) fn current_delay_samples(&self) -> f32 {
        self.shifter.delay_samples()
    }

    /// Worst case algorithmic delay of the chain in samples: the longest grain
    /// the shifter can grow to plus its guard delay. Used by the latency test to
    /// keep the chain honest.
    #[cfg(test)]
    pub(super) fn algorithmic_delay_samples() -> usize {
        super::pitch::max_delay_samples() as usize
    }

    #[cfg(test)]
    pub(super) fn algorithmic_delay_ms() -> f32 {
        Self::algorithmic_delay_samples() as f32 * 1_000.0 / DEFAULT_SAMPLE_RATE as f32
    }
}

fn semitone_ratio(semitones: f32) -> f32 {
    (semitones / 12.0).exp2()
}
