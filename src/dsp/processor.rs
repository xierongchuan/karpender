use crate::config::VoiceMode;

use super::{
    DEFAULT_SAMPLE_RATE, PITCH_BUFFER, PITCH_WINDOW, TWO_PI,
    math::{
        advance_pitch_phase, effective_privacy, input_drive, raised_cosine, smoothstep, soft_clip,
    },
    noise::{initial_jitter_seed, xorshift32},
    params::SharedDspParams,
    recipe::pitch_recipe,
};

const MIN_EXPANDER_GAIN: f32 = 0.48;

#[derive(Debug)]
pub struct VoiceProcessor {
    pub(super) pitch_buffer: Vec<f32>,
    write_pos: usize,
    down_phase: f32,
    up_phase: f32,
    body_state: f32,
    speech_state: f32,
    prosody_state: f32,
    formant_state: f32,
    consonant_state: f32,
    expander_envelope: f32,
    expander_gain: f32,
    compressor_envelope: f32,
    doubler_phase: f32,
    color_phase: f32,
    jitter_value: f32,
    jitter_timer: u32,
    pub(super) jitter_seed: u32,
    pub(super) noise_seed: u32,
    noise_state: f32,
    fricative_state: f32,
    morph_state: f32,
    accent_value: f32,
    accent_timer: u32,
    output_envelope: f32,
    output_gain: f32,
    allpass_x1: f32,
    allpass_y1: f32,
}

impl Default for VoiceProcessor {
    fn default() -> Self {
        Self {
            pitch_buffer: vec![0.0; PITCH_BUFFER],
            write_pos: 0,
            down_phase: 0.0,
            up_phase: 0.5,
            body_state: 0.0,
            speech_state: 0.0,
            prosody_state: 0.0,
            formant_state: 0.0,
            consonant_state: 0.0,
            expander_envelope: 0.0,
            expander_gain: 1.0,
            compressor_envelope: 0.0,
            doubler_phase: 0.0,
            color_phase: 0.0,
            jitter_value: 0.0,
            jitter_timer: 0,
            jitter_seed: initial_jitter_seed(),
            noise_seed: initial_jitter_seed() ^ 0x85eb_ca6b,
            noise_state: 0.0,
            fricative_state: 0.0,
            morph_state: 0.0,
            accent_value: 0.0,
            accent_timer: 0,
            output_envelope: 0.0,
            output_gain: 1.0,
            allpass_x1: 0.0,
            allpass_y1: 0.0,
        }
    }
}

impl VoiceProcessor {
    pub fn process_sample(&mut self, input: f32, params: &SharedDspParams) -> f32 {
        let params = params.snapshot();
        let privacy = effective_privacy(params.robot_amount);
        let monotone_privacy = if params.monotone {
            privacy.max(0.35)
        } else {
            privacy
        };
        let timbre_jitter = self.next_timbre_jitter(params.voice_mode, monotone_privacy);
        let accent = self.next_accent(params.voice_mode, monotone_privacy);
        let cleaned = self.noise_cleanup(input * input_drive(params.gain), params.noise_gate);

        let (down_ratio, up_ratio, down_weight, pitch_mix) =
            pitch_recipe(params.voice_mode, monotone_privacy, timbre_jitter, accent);
        self.pitch_buffer[self.write_pos] = cleaned;
        let shifted_down = self.pitch_tap_pair(down_ratio, self.down_phase);
        let shifted_up = self.pitch_tap_pair(up_ratio, self.up_phase);
        self.down_phase = advance_pitch_phase(down_ratio, self.down_phase);
        self.up_phase = advance_pitch_phase(up_ratio, self.up_phase);
        self.write_pos = (self.write_pos + 1) % self.pitch_buffer.len();

        let shifted = shifted_down * down_weight + shifted_up * (1.0 - down_weight);
        let pitched = cleaned * (1.0 - pitch_mix) + shifted * pitch_mix;
        let rotated =
            self.phase_rotate(pitched, monotone_privacy, params.voice_mode, timbre_jitter);
        let disguised = self.identity_mask(
            rotated,
            monotone_privacy,
            params.voice_mode,
            timbre_jitter,
            accent,
        );
        let morphed = self.deep_morph(
            disguised,
            monotone_privacy,
            params.voice_mode,
            timbre_jitter,
            accent,
        );
        let mut sample = self.speech_compress(morphed, monotone_privacy);
        let flatten_amount = match params.voice_mode {
            VoiceMode::Masked => monotone_privacy * 0.30,
            VoiceMode::BrightStranger => monotone_privacy * 0.36,
            VoiceMode::DeepMorph => 0.28 + monotone_privacy * 0.40,
            VoiceMode::CinematicHigh => 0.50 + monotone_privacy * 0.42,
        };
        sample = self.flatten_personality(sample, flatten_amount);

        if params.monotone {
            sample = self.flatten_personality(sample, 0.40 + monotone_privacy * 0.40);
        }

        let leveled = self.stabilize_loudness(sample, params.voice_mode);

        soft_clip(leveled).clamp(-1.0, 1.0)
    }

    fn noise_cleanup(&mut self, input: f32, threshold: f32) -> f32 {
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

        let expanded = input * self.expander_gain;
        input * (1.0 - strength * 0.22) + expanded * strength * 0.22
    }

    fn pitch_tap_pair(&self, ratio: f32, phase: f32) -> f32 {
        let first = self.pitch_tap(ratio, phase);
        let second_phase = (phase + 0.5).fract();
        let second = self.pitch_tap(ratio, second_phase);
        let fade = raised_cosine(phase);

        first * fade + second * (1.0 - fade)
    }

    fn pitch_tap(&self, ratio: f32, phase: f32) -> f32 {
        let delay = if ratio < 1.0 {
            phase * PITCH_WINDOW as f32
        } else {
            (1.0 - phase) * PITCH_WINDOW as f32
        } + 64.0;

        self.read_delay(delay)
    }

    pub(super) fn read_delay(&self, delay: f32) -> f32 {
        let len = self.pitch_buffer.len() as f32;
        let read = (self.write_pos as f32 - delay).rem_euclid(len);
        let floor = read.floor();
        let i0 = floor as usize % self.pitch_buffer.len();
        let i1 = (i0 + 1) % self.pitch_buffer.len();
        let frac = read - floor;

        self.pitch_buffer[i0] * (1.0 - frac) + self.pitch_buffer[i1] * frac
    }

    fn identity_mask(
        &mut self,
        input: f32,
        privacy: f32,
        mode: VoiceMode,
        timbre_jitter: f32,
        accent: f32,
    ) -> f32 {
        self.body_state += 0.026 * (input - self.body_state);
        self.formant_state += 0.075 * (input - self.formant_state);
        self.speech_state += 0.18 * (input - self.speech_state);
        self.consonant_state += 0.42 * (input - self.consonant_state);
        let body = self.body_state;
        let low_mids = self.formant_state - self.body_state;
        let high_mids = self.speech_state - self.formant_state;
        let consonants = input - self.consonant_state;

        self.color_phase += TWO_PI * (0.09 + privacy * 0.08) / DEFAULT_SAMPLE_RATE as f32;
        if self.color_phase >= TWO_PI {
            self.color_phase -= TWO_PI;
        }
        let color = self.color_phase.sin();

        let mode_brightness = match mode {
            VoiceMode::Masked => 0.0,
            VoiceMode::BrightStranger => 1.0,
            VoiceMode::DeepMorph => 1.7,
            VoiceMode::CinematicHigh => 2.5,
        };

        let short_delay = self.read_delay(
            300.0 + privacy * 360.0 + color * 45.0 + timbre_jitter * 70.0 + accent * 95.0,
        );
        self.doubler_phase += TWO_PI * (0.31 + privacy * 0.17) / DEFAULT_SAMPLE_RATE as f32;
        if self.doubler_phase >= TWO_PI {
            self.doubler_phase -= TWO_PI;
        }
        let moving_delay = self.read_delay(
            620.0 + self.doubler_phase.sin() * 130.0 + timbre_jitter * 95.0 - accent * 120.0,
        );
        let doubler = (short_delay * 0.58 + moving_delay * 0.42)
            * (0.18 + privacy * 0.34 + mode_brightness * 0.06 + accent.abs() * 0.06);

        let jitter_color = timbre_jitter * privacy;
        let body_weight = 0.70 - privacy * (0.28 + mode_brightness * 0.12)
            + color * privacy * 0.08
            + jitter_color * 0.08
            + accent * 0.10;
        let low_mid_weight = 0.54 - privacy * (0.34 + mode_brightness * 0.08);
        let high_mid_weight = 0.42
            - privacy * (0.28 - mode_brightness * 0.10)
            - color * privacy * 0.06
            - jitter_color * 0.07
            - accent * 0.12;
        let consonant_weight =
            1.12 + privacy * (0.36 + mode_brightness * 0.18) + accent.abs() * 0.18;
        let nasal_offset = (low_mids - high_mids)
            * privacy
            * (0.18 + color * 0.07 + jitter_color * 0.06 + accent * 0.09);

        let masked = body * body_weight
            + low_mids * low_mid_weight
            + high_mids * high_mid_weight
            + consonants * consonant_weight
            + nasal_offset
            + doubler;
        let mix = (privacy * 0.88).min(0.92);

        input * (1.0 - mix) + masked * mix
    }

    fn deep_morph(
        &mut self,
        input: f32,
        privacy: f32,
        mode: VoiceMode,
        timbre_jitter: f32,
        accent: f32,
    ) -> f32 {
        let depth = match mode {
            VoiceMode::Masked => 0.18,
            VoiceMode::BrightStranger => 0.38,
            VoiceMode::DeepMorph => 1.0,
            VoiceMode::CinematicHigh => 1.35,
        } * privacy;

        if depth <= 0.001 {
            return input;
        }

        self.morph_state += 0.052 * (input - self.morph_state);
        let morph_band = input - self.morph_state;
        let warped = self.morph_state * (0.62 - depth * 0.24)
            + morph_band * (1.18 + depth * 0.42 + timbre_jitter * 0.08 + accent * 0.10);

        let consonant_energy = (input - self.consonant_state).abs();
        self.fricative_state += 0.18 * (consonant_energy - self.fricative_state);
        let transient = (consonant_energy - self.fricative_state * 1.55).max(0.0);
        let fricative_gate = smoothstep((transient * 18.0).clamp(0.0, 1.0));
        let consonant_rebuild = self.next_soft_noise()
            * transient.min(0.16)
            * fricative_gate
            * depth
            * (0.045 + accent.abs() * 0.025);
        let shimmer = self
            .read_delay(110.0 + 170.0 * privacy + timbre_jitter * 55.0 + accent * 70.0)
            * depth
            * (0.12 + accent.abs() * 0.06);
        let accent_push = input * accent * depth * 0.18;

        warped * (1.0 - depth * 0.10) + consonant_rebuild + shimmer + accent_push
    }

    fn phase_rotate(
        &mut self,
        input: f32,
        privacy: f32,
        mode: VoiceMode,
        timbre_jitter: f32,
    ) -> f32 {
        let mode_offset = match mode {
            VoiceMode::Masked => 0.0,
            VoiceMode::BrightStranger => 0.10,
            VoiceMode::DeepMorph => 0.20,
            VoiceMode::CinematicHigh => 0.30,
        };
        let coefficient = (0.30
            + privacy * (0.38 + mode_offset)
            + self.color_phase.sin() * privacy * 0.04
            + timbre_jitter * privacy * 0.07)
            .clamp(0.05, 0.86);
        let rotated = -coefficient * input + self.allpass_x1 + coefficient * self.allpass_y1;
        self.allpass_x1 = input;
        self.allpass_y1 = rotated;

        let amount = privacy * (0.26 + mode_offset * 0.35);
        input * (1.0 - amount) + rotated * amount
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
            1.0 + privacy * 0.08
        };
        let amount = privacy * 0.65;
        let applied_gain = 1.0 * (1.0 - amount) + gain * amount;

        input * applied_gain
    }

    fn flatten_personality(&mut self, input: f32, amount: f32) -> f32 {
        let amount = amount.clamp(0.0, 0.72);
        self.prosody_state += 0.0045 * (input - self.prosody_state);

        let flattened = input - self.prosody_state * amount;
        let clarity = flattened + (input - self.speech_state) * (0.06 + amount * 0.04);

        clarity * (1.0 + amount * 0.08)
    }

    fn next_timbre_jitter(&mut self, mode: VoiceMode, privacy: f32) -> f32 {
        if self.jitter_timer == 0 {
            self.jitter_seed = xorshift32(self.jitter_seed);
            let unit = (self.jitter_seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let range = match mode {
                VoiceMode::Masked => 0.32,
                VoiceMode::BrightStranger => 0.55,
                VoiceMode::DeepMorph => 0.82,
                VoiceMode::CinematicHigh => 1.05,
            };
            self.jitter_value = unit * privacy * range;
            self.jitter_timer = match mode {
                VoiceMode::Masked => DEFAULT_SAMPLE_RATE / 16,
                VoiceMode::BrightStranger => DEFAULT_SAMPLE_RATE / 22,
                VoiceMode::DeepMorph => DEFAULT_SAMPLE_RATE / 30,
                VoiceMode::CinematicHigh => DEFAULT_SAMPLE_RATE / 24,
            };
        } else {
            self.jitter_timer -= 1;
        }

        self.jitter_value
    }

    fn next_accent(&mut self, mode: VoiceMode, privacy: f32) -> f32 {
        if self.accent_timer == 0 {
            self.jitter_seed = xorshift32(self.jitter_seed ^ 0x27d4_eb2d);
            let unit = (self.jitter_seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let amount = match mode {
                VoiceMode::Masked => 0.10,
                VoiceMode::BrightStranger => 0.18,
                VoiceMode::DeepMorph => 0.28,
                VoiceMode::CinematicHigh => 0.38,
            };
            self.accent_value = unit * privacy * amount;
            let base = match mode {
                VoiceMode::Masked => DEFAULT_SAMPLE_RATE / 9,
                VoiceMode::BrightStranger => DEFAULT_SAMPLE_RATE / 11,
                VoiceMode::DeepMorph => DEFAULT_SAMPLE_RATE / 13,
                VoiceMode::CinematicHigh => DEFAULT_SAMPLE_RATE / 15,
            };
            let spread = (self.jitter_seed % (base / 2).max(1)).max(1);
            self.accent_timer = base + spread;
        } else {
            self.accent_timer -= 1;
        }

        self.accent_value
    }

    fn stabilize_loudness(&mut self, input: f32, mode: VoiceMode) -> f32 {
        let amplitude = input.abs();
        let coeff = if amplitude > self.output_envelope {
            0.035
        } else {
            0.004
        };
        self.output_envelope += coeff * (amplitude - self.output_envelope);

        let target = match mode {
            VoiceMode::Masked => 0.22,
            VoiceMode::BrightStranger => 0.23,
            VoiceMode::DeepMorph => 0.24,
            VoiceMode::CinematicHigh => 0.24,
        };
        let desired = if self.output_envelope > 0.018 {
            (target / self.output_envelope).clamp(0.45, 1.75)
        } else {
            1.0
        };
        let gain_coeff = if desired < self.output_gain {
            0.018
        } else {
            0.003
        };
        self.output_gain += gain_coeff * (desired - self.output_gain);

        input * self.output_gain
    }

    pub(super) fn next_soft_noise(&mut self) -> f32 {
        self.noise_seed = xorshift32(self.noise_seed);
        let white = (self.noise_seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
        self.noise_state += 0.12 * (white - self.noise_state);
        self.noise_state
    }
}
