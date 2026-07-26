use crate::config::VoiceMode;

use super::{
    DEFAULT_SAMPLE_RATE, MAX_GRAIN, MIN_GRAIN, PITCH_BUFFER, SharedDspParams, TWO_PI,
    VoiceProcessor,
    formant::{BAND_COUNT, FormantShaper, band_center},
    pitch::{PitchShifter, advance_phase, grain_window},
    recipe::voice_recipe,
    tracker::PitchTracker,
};

/// Frequency estimate from the sign changes of a block of samples.
fn zero_crossing_hz(samples: &[f32]) -> f32 {
    let crossings = samples
        .windows(2)
        .filter(|pair| (pair[0] <= 0.0) != (pair[1] <= 0.0))
        .count();

    crossings as f32 * DEFAULT_SAMPLE_RATE as f32 / (2.0 * samples.len() as f32)
}

/// Magnitude of a single frequency bin, computed directly. Zero crossings are
/// too coarse for the shifter tests because the crossfade of a delay line
/// shifter adds a slow warble around the true frequency.
fn bin_magnitude(samples: &[f32], frequency: f32) -> f32 {
    let mut real = 0.0_f32;
    let mut imaginary = 0.0_f32;
    for (index, value) in samples.iter().enumerate() {
        let angle = TWO_PI * frequency * index as f32 / DEFAULT_SAMPLE_RATE as f32;
        real += value * angle.cos();
        imaginary += value * angle.sin();
    }

    (real * real + imaginary * imaginary).sqrt() / samples.len() as f32
}

/// Strongest frequency inside a range, scanned at one hertz resolution.
fn dominant_hz(samples: &[f32], low: f32, high: f32) -> f32 {
    let mut best = low;
    let mut best_magnitude = 0.0_f32;
    let mut frequency = low;
    while frequency <= high {
        let magnitude = bin_magnitude(samples, frequency);
        if magnitude > best_magnitude {
            best_magnitude = magnitude;
            best = frequency;
        }
        frequency += 1.0;
    }

    best
}

fn sine(frequency: f32, index: usize, amplitude: f32) -> f32 {
    (TWO_PI * frequency * index as f32 / DEFAULT_SAMPLE_RATE as f32).sin() * amplitude
}

/// Two harmonics plus a formant like resonance, so the pitch tracker and the
/// envelope shaper see something closer to speech than a bare sine.
fn voiced(frequency: f32, index: usize, amplitude: f32) -> f32 {
    let base = sine(frequency, index, 1.0);
    let second = sine(frequency * 2.0, index, 0.5);
    let third = sine(frequency * 3.0, index, 0.28);
    let formant = sine(frequency * 6.0, index, 0.16);

    (base + second + third + formant) * amplitude * 0.5
}

fn run(
    processor: &mut VoiceProcessor,
    params: &SharedDspParams,
    frequency: f32,
    samples: usize,
) -> Vec<f32> {
    let mut output = Vec::with_capacity(samples);
    for index in 0..samples {
        output.push(processor.process_sample(voiced(frequency, index, 0.5), params));
    }

    output
}

// --- pitch shifter ---------------------------------------------------------

#[test]
fn unit_ratio_leaves_the_delay_untouched() {
    // Regression: the old phase advance forced a minimum shift of eight
    // percent, so even a neutral setting warbled the voice.
    let mut phase = 0.25;
    for _ in 0..10_000 {
        phase = advance_phase(phase, 1.0, MIN_GRAIN);
    }

    assert_eq!(phase, 0.25);
}

#[test]
fn phase_advance_direction_follows_the_ratio() {
    assert!(
        advance_phase(0.5, 0.8, MIN_GRAIN) > 0.5,
        "down shift lengthens the delay"
    );
    assert!(
        advance_phase(0.5, 1.2, MIN_GRAIN) < 0.5,
        "up shift shortens the delay"
    );
}

#[test]
fn grain_tracks_two_periods_of_the_detected_pitch() {
    // A 150 Hz voice has a 320 sample period, so the grain settles at 640.
    assert_eq!(grain_window(320.0), 640.0);
    // Unvoiced input has no period to lock to, and neither extreme escapes the
    // bounds that keep the delay bounded.
    assert_eq!(grain_window(0.0), MIN_GRAIN);
    assert_eq!(grain_window(40.0), MIN_GRAIN);
    assert_eq!(grain_window(4_000.0), MAX_GRAIN);
}

#[test]
fn unit_ratio_passes_the_signal_through() {
    let mut shifter = PitchShifter::default();
    let grain = grain_window(DEFAULT_SAMPLE_RATE as f32 / 200.0);
    shifter.set_grain(grain);
    let mut output = Vec::new();
    for index in 0..4_096 {
        output.push(shifter.process(sine(200.0, index, 0.4), 1.0, grain));
    }

    let tail = &output[2_048..];
    let peak = tail
        .iter()
        .fold(0.0_f32, |peak, value| peak.max(value.abs()));

    assert!(peak > 0.3, "expected the tone to survive, peak was {peak}");
    assert!((zero_crossing_hz(tail) - 200.0).abs() < 6.0);
}

#[test]
fn up_shift_raises_the_measured_frequency() {
    // Regression: with a grain that ignored the input period the delay ramp
    // wrapped mid cycle, so 150 Hz times 1.5 landed on the nearest line of the
    // comb, 212 Hz, instead of 225 Hz. Every harmonic picked up a different
    // error, which is what made the output inharmonic and squeaky.
    let mut shifter = PitchShifter::default();
    let grain = grain_window(DEFAULT_SAMPLE_RATE as f32 / 150.0);
    shifter.set_grain(grain);
    let mut output = Vec::new();
    for index in 0..8_192 {
        output.push(shifter.process(sine(150.0, index, 0.4), 1.5, grain));
    }

    let measured = dominant_hz(&output[4_096..], 100.0, 320.0);

    assert!(
        (measured - 225.0).abs() < 6.0,
        "expected about 225 Hz, measured {measured}"
    );
}

#[test]
fn down_shift_lowers_the_measured_frequency() {
    let mut shifter = PitchShifter::default();
    let grain = grain_window(DEFAULT_SAMPLE_RATE as f32 / 200.0);
    shifter.set_grain(grain);
    let mut output = Vec::new();
    for index in 0..8_192 {
        output.push(shifter.process(sine(200.0, index, 0.4), 0.75, grain));
    }

    let measured = dominant_hz(&output[4_096..], 80.0, 260.0);

    assert!(
        (measured - 150.0).abs() < 6.0,
        "expected about 150 Hz, measured {measured}"
    );
}

#[test]
fn delay_reader_wraps_exact_buffer_boundary() {
    let mut shifter = PitchShifter::default();
    shifter.buffer_mut()[0] = 0.25;
    shifter.buffer_mut()[1] = 0.5;
    shifter.set_write_pos(0);

    assert_eq!(shifter.read_delay(PITCH_BUFFER as f32), 0.25);
}

// --- pitch tracker ---------------------------------------------------------

#[test]
fn tracker_finds_the_fundamental() {
    for target in [95.0_f32, 140.0, 220.0] {
        let mut tracker = PitchTracker::default();
        for index in 0..DEFAULT_SAMPLE_RATE as usize / 2 {
            tracker.observe(voiced(target, index, 0.5));
        }

        let measured = tracker.frequency();
        assert!(
            (measured - target).abs() < target * 0.06,
            "expected {target} Hz, tracked {measured} Hz"
        );
    }
}

#[test]
fn tracker_reports_silence_as_unvoiced() {
    let mut tracker = PitchTracker::default();
    for _ in 0..DEFAULT_SAMPLE_RATE as usize / 2 {
        tracker.observe(0.0);
    }

    assert_eq!(tracker.frequency(), 0.0);
    assert_eq!(tracker.confidence(), 0.0);
}

// --- formant shaper --------------------------------------------------------

#[test]
fn shaper_moves_the_spectral_envelope_upwards() {
    // A tone parked on band two should push the cascade to boost band three
    // when the envelope is asked to move up by one band.
    let mut shaper = FormantShaper::default();
    let center = band_center(2);
    let mut color = [0.0_f32; BAND_COUNT];
    color[0] = 0.0;

    for index in 0..DEFAULT_SAMPLE_RATE as usize / 4 {
        shaper.observe(sine(center, index, 0.5));
    }
    shaper.update(1.62, 1.0, 0.0, &color);

    let low = shaper.gain_db(2);
    let high = shaper.gain_db(3);

    assert!(
        high > low,
        "band above the tone should be lifted, got {high} vs {low}"
    );
}

#[test]
fn neutral_shaper_stays_flat() {
    let mut shaper = FormantShaper::default();
    let color = [0.0_f32; BAND_COUNT];
    for index in 0..DEFAULT_SAMPLE_RATE as usize / 4 {
        shaper.observe(sine(band_center(3), index, 0.5));
    }
    for _ in 0..64 {
        shaper.update(1.0, 1.0, 0.0, &color);
    }

    for band in 0..BAND_COUNT {
        assert!(
            shaper.gain_db(band).abs() < 0.5,
            "band {band} drifted to {} dB",
            shaper.gain_db(band)
        );
    }
}

// --- latency ---------------------------------------------------------------

#[test]
fn worst_case_algorithmic_delay_stays_bounded() {
    // Issue 1: the active mode was noticeably late. The grain is the dominant
    // contributor, and only the deepest voice pays the maximum, so both the
    // ceiling and the typical case are pinned by tests.
    let delay = VoiceProcessor::algorithmic_delay_ms();

    assert!(delay < 25.0, "worst case delay grew to {delay} ms");
    assert_eq!(
        VoiceProcessor::algorithmic_delay_samples(),
        MAX_GRAIN as usize + 24
    );
}

#[test]
fn a_typical_voice_pays_well_under_fifteen_milliseconds() {
    let params = SharedDspParams::new();
    let mut processor = VoiceProcessor::seeded(0x51ce_5eed);
    run(&mut processor, &params, 150.0, DEFAULT_SAMPLE_RATE as usize);

    let delay_ms = processor.current_delay_samples() * 1_000.0 / DEFAULT_SAMPLE_RATE as f32;

    assert!(
        delay_ms < 15.0,
        "a 150 Hz voice should settle near 14 ms, measured {delay_ms} ms"
    );
}

// --- voice conversion ------------------------------------------------------

#[test]
fn low_modes_pull_a_high_voice_down() {
    let params = SharedDspParams::new();
    params.set_robot_amount(1.0);
    params.set_voice_mode(VoiceMode::LowBaritone);

    let mut processor = VoiceProcessor::seeded(0x1234_5678);
    run(
        &mut processor,
        &params,
        230.0,
        DEFAULT_SAMPLE_RATE as usize / 2,
    );

    assert!(
        processor.pitch_ratio() < 0.85,
        "expected a downward shift, ratio was {}",
        processor.pitch_ratio()
    );
}

#[test]
fn high_modes_pull_a_low_voice_up() {
    let params = SharedDspParams::new();
    params.set_robot_amount(1.0);
    params.set_voice_mode(VoiceMode::CinematicHigh);

    let mut processor = VoiceProcessor::seeded(0x1234_5678);
    run(
        &mut processor,
        &params,
        92.0,
        DEFAULT_SAMPLE_RATE as usize / 2,
    );

    assert!(
        processor.pitch_ratio() > 1.15,
        "expected an upward shift, ratio was {}",
        processor.pitch_ratio()
    );
}

#[test]
fn every_mode_lands_near_its_target_pitch() {
    // The point of the rewrite: modes differ because they normalise to
    // different fundamentals, not because they add different amounts of fizz.
    for mode in VoiceMode::ALL {
        let params = SharedDspParams::new();
        params.set_robot_amount(1.0);
        params.set_monotone(true);
        params.set_voice_mode(mode);

        let mut processor = VoiceProcessor::seeded(0x2222_3333);
        run(
            &mut processor,
            &params,
            155.0,
            DEFAULT_SAMPLE_RATE as usize / 2,
        );

        let recipe = voice_recipe(mode);
        let produced = 155.0 * processor.pitch_ratio();
        let error = (produced - recipe.target_f0).abs() / recipe.target_f0;

        assert!(
            error < 0.22,
            "{mode:?} produced {produced:.1} Hz for a target of {:.1} Hz",
            recipe.target_f0
        );
    }
}

#[test]
fn modes_do_not_collapse_onto_the_same_voice() {
    let mut ratios = Vec::new();
    for mode in VoiceMode::ALL {
        let params = SharedDspParams::new();
        params.set_robot_amount(1.0);
        params.set_monotone(true);
        params.set_voice_mode(mode);

        let mut processor = VoiceProcessor::seeded(0x4444_5555);
        run(
            &mut processor,
            &params,
            150.0,
            DEFAULT_SAMPLE_RATE as usize / 2,
        );
        ratios.push(processor.pitch_ratio());
    }

    let lowest = ratios.iter().copied().fold(f32::INFINITY, f32::min);
    let highest = ratios.iter().copied().fold(f32::NEG_INFINITY, f32::max);

    assert!(
        highest / lowest > 1.7,
        "modes only spanned {lowest:.2} to {highest:.2}"
    );
}

#[test]
fn transparent_settings_keep_the_voice_close_to_the_original() {
    let params = SharedDspParams::new();
    params.set_robot_amount(0.0);
    params.set_noise_gate(0.0);

    let mut processor = VoiceProcessor::seeded(0x9999_1111);
    let output = run(
        &mut processor,
        &params,
        150.0,
        DEFAULT_SAMPLE_RATE as usize / 2,
    );
    let tail = &output[output.len() - 8_192..];

    assert!(
        (processor.pitch_ratio() - 1.0).abs() < 0.02,
        "privacy zero should barely move the pitch, ratio {}",
        processor.pitch_ratio()
    );
    assert!((zero_crossing_hz(tail) - zero_crossing_hz(&output[8_192..16_384])).abs() < 40.0);
}

// --- anti fingerprinting ---------------------------------------------------

#[test]
fn two_sessions_produce_two_different_voices() {
    // Issue 3: the same speaker on two runs must not map to one stable
    // synthetic identity, otherwise the mapping itself is a fingerprint.
    let params = SharedDspParams::new();
    params.set_robot_amount(1.0);

    let mut first = VoiceProcessor::seeded(0x1111_1111);
    let mut second = VoiceProcessor::seeded(0xaaaa_bbbb);
    run(&mut first, &params, 150.0, DEFAULT_SAMPLE_RATE as usize / 2);
    run(
        &mut second,
        &params,
        150.0,
        DEFAULT_SAMPLE_RATE as usize / 2,
    );

    let spread = (first.pitch_ratio() / second.pitch_ratio() - 1.0).abs();

    assert!(
        spread > 0.02,
        "session identities were too close: {} vs {}",
        first.pitch_ratio(),
        second.pitch_ratio()
    );
}

#[test]
fn different_speakers_converge_on_the_same_pitch() {
    // The flip side: two different voices in the same mode should come out at
    // the same fundamental, which is what removes the speaker cue.
    let params = SharedDspParams::new();
    params.set_robot_amount(1.0);
    params.set_monotone(true);
    params.set_voice_mode(VoiceMode::CalmAndrogynous);

    let mut low = VoiceProcessor::seeded(0x5555_5555);
    let mut high = VoiceProcessor::seeded(0x5555_5555);
    run(&mut low, &params, 105.0, DEFAULT_SAMPLE_RATE as usize / 2);
    run(&mut high, &params, 205.0, DEFAULT_SAMPLE_RATE as usize / 2);

    let low_output = 105.0 * low.pitch_ratio();
    let high_output = 205.0 * high.pitch_ratio();

    assert!(
        (low_output - high_output).abs() < 28.0,
        "expected both speakers near one target, got {low_output:.1} and {high_output:.1}"
    );
}

// --- levels and safety -----------------------------------------------------

#[test]
fn cleanup_attenuates_sustained_quiet_noise() {
    // The expander is measured on its own rather than through the whole chain:
    // the loudness stabiliser downstream deliberately pulls every level towards
    // the same target, so an end to end sum mostly reports what the automatic
    // gain did and only faintly what the gate did.
    let mut quiet = VoiceProcessor::seeded(0x1010_1010);
    let mut loud = VoiceProcessor::seeded(0x1010_1010);
    let mut quiet_sum = 0.0;
    let mut loud_sum = 0.0;
    for index in 0..DEFAULT_SAMPLE_RATE as usize / 4 {
        quiet_sum += quiet.noise_cleanup(sine(150.0, index, 0.01), 0.1).abs();
        loud_sum += loud.noise_cleanup(sine(150.0, index, 0.25), 0.1).abs();
    }

    assert!(
        quiet_sum / 0.01 < loud_sum / 0.25 * 0.75,
        "the expander did not push the quiet signal down"
    );
}

#[test]
fn loud_speech_survives_a_high_gate() {
    let params = SharedDspParams::new();
    params.set_noise_gate(0.4);
    params.set_robot_amount(0.0);

    let mut processor = VoiceProcessor::seeded(0x2020_2020);
    let output = run(
        &mut processor,
        &params,
        150.0,
        DEFAULT_SAMPLE_RATE as usize / 4,
    );
    let peak = output[output.len() - 4_096..]
        .iter()
        .fold(0.0_f32, |peak, value| peak.max(value.abs()));

    assert!(peak > 0.08, "voice was gated away, peak {peak}");
}

#[test]
fn gain_is_clamped() {
    let params = SharedDspParams::new();
    params.set_gain(10.0);

    let mut processor = VoiceProcessor::seeded(0x3030_3030);
    for index in 0..4_096 {
        let output = processor.process_sample(voiced(150.0, index, 0.9), &params);
        assert!(output.is_finite());
        assert!(output.abs() <= 1.0);
    }
}

#[test]
fn output_stays_finite_for_extreme_settings() {
    for mode in VoiceMode::ALL {
        let params = SharedDspParams::new();
        params.set_gain(4.0);
        params.set_noise_gate(1.0);
        params.set_robot_amount(1.0);
        params.set_monotone(true);
        params.set_voice_mode(mode);

        let mut processor = VoiceProcessor::seeded(0x4040_4040);
        for index in 0..DEFAULT_SAMPLE_RATE as usize / 8 {
            let input = voiced(120.0, index, 0.95) + if index % 997 == 0 { 1.0 } else { 0.0 };
            let output = processor.process_sample(input, &params);
            assert!(output.is_finite(), "{mode:?} produced a non finite sample");
            assert!(output.abs() <= 1.0);
        }
    }
}

#[test]
fn silence_in_silence_out() {
    let params = SharedDspParams::new();
    params.set_robot_amount(1.0);

    let mut processor = VoiceProcessor::seeded(0x5050_5050);
    let mut peak = 0.0_f32;
    for _ in 0..DEFAULT_SAMPLE_RATE as usize / 4 {
        peak = peak.max(processor.process_sample(0.0, &params).abs());
    }

    assert!(peak < 0.01, "silence leaked {peak}");
}

#[test]
fn synthetic_breath_noise_is_smoothed() {
    let mut processor = VoiceProcessor::seeded(0x6060_6060);
    let mut previous = processor.next_soft_noise();
    let mut max_delta = 0.0_f32;
    for _ in 0..512 {
        let current = processor.next_soft_noise();
        max_delta = max_delta.max((current - previous).abs());
        previous = current;
    }

    assert!(max_delta < 0.30);
}
