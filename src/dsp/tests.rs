use crate::config::VoiceMode;

use super::{
    DEFAULT_SAMPLE_RATE, PITCH_BUFFER, PITCH_WINDOW, SharedDspParams, TWO_PI, VoiceProcessor,
};

#[test]
fn cleanup_attenuates_sustained_quiet_noise() {
    let params = SharedDspParams::new();
    params.set_noise_gate(0.1);

    let mut processor = VoiceProcessor::default();
    let mut output = 0.0;
    for _ in 0..4000 {
        output = processor.process_sample(0.02, &params);
    }

    assert!(output.abs() < 0.02);
    assert!(output.abs() > 0.0);
}

#[test]
fn high_cleanup_preserves_sustained_voice() {
    let params = SharedDspParams::new();
    params.set_noise_gate(0.4);
    params.set_robot_amount(0.0);

    let mut processor = VoiceProcessor::default();
    let mut output = 0.0;
    for _ in 0..4000 {
        output = processor.process_sample(0.2, &params);
    }

    assert!(output.abs() > 0.12);
}

#[test]
fn gain_is_clamped() {
    let params = SharedDspParams::new();
    params.set_gain(10.0);
    params.set_robot_amount(0.0);

    let mut processor = VoiceProcessor::default();
    let output = processor.process_sample(0.5, &params);

    assert!(output.is_finite());
    assert!(output.abs() <= 1.0);
}

#[test]
fn gain_changes_drive_without_linear_output_volume() {
    let low_gain = SharedDspParams::new();
    low_gain.set_robot_amount(0.75);

    let high_gain = SharedDspParams::new();
    high_gain.set_gain(4.0);
    high_gain.set_robot_amount(0.75);

    let mut low_processor = seeded_processor();
    let mut high_processor = seeded_processor();
    let mut low_sum = 0.0;
    let mut high_sum = 0.0;
    for n in 0..PITCH_WINDOW * 4 {
        let sample = (TWO_PI * 160.0 * n as f32 / DEFAULT_SAMPLE_RATE as f32).sin() * 0.18;
        let low = low_processor.process_sample(sample, &low_gain).abs();
        let high = high_processor.process_sample(sample, &high_gain).abs();
        if n >= PITCH_WINDOW {
            low_sum += low;
            high_sum += high;
        }
    }

    assert!(high_sum > low_sum * 0.85);
    assert!(high_sum < low_sum * 1.9);
}

#[test]
fn privacy_chain_changes_sustained_voice() {
    let params = SharedDspParams::new();
    params.set_robot_amount(1.0);

    let mut processor = VoiceProcessor::default();
    let mut output = 0.0;
    for _ in 0..PITCH_WINDOW {
        output = processor.process_sample(0.3, &params);
    }

    assert_ne!(output, 0.3);
}

#[test]
fn default_privacy_changes_more_than_plain_gain() {
    let params = SharedDspParams::new();

    let mut processor = VoiceProcessor::default();
    let mut output = 0.0;
    for n in 0..PITCH_WINDOW {
        let sample = (TWO_PI * 140.0 * n as f32 / DEFAULT_SAMPLE_RATE as f32).sin() * 0.22;
        output = processor.process_sample(sample, &params);
    }

    assert!(output.abs() > 0.0);
    assert_ne!(output, 0.22);
}

#[test]
fn bright_stranger_uses_distinct_processing_path() {
    let masked = SharedDspParams::new();
    masked.set_robot_amount(0.75);

    let bright = SharedDspParams::new();
    bright.set_robot_amount(0.75);
    bright.set_voice_mode(VoiceMode::BrightStranger);

    let mut masked_processor = seeded_processor();
    let mut bright_processor = seeded_processor();
    let mut masked_sum = 0.0;
    let mut bright_sum = 0.0;
    for n in 0..PITCH_WINDOW {
        let sample = (TWO_PI * 160.0 * n as f32 / DEFAULT_SAMPLE_RATE as f32).sin() * 0.24;
        masked_sum += masked_processor.process_sample(sample, &masked).abs();
        bright_sum += bright_processor.process_sample(sample, &bright).abs();
    }

    assert!((masked_sum - bright_sum).abs() > 0.01);
}

#[test]
fn deep_morph_is_distinct_and_preserves_voice_energy() {
    let bright = SharedDspParams::new();
    bright.set_robot_amount(0.9);
    bright.set_voice_mode(VoiceMode::BrightStranger);

    let deep = SharedDspParams::new();
    deep.set_robot_amount(0.9);
    deep.set_monotone(true);
    deep.set_voice_mode(VoiceMode::DeepMorph);

    let mut bright_processor = seeded_processor();
    let mut deep_processor = seeded_processor();
    let mut bright_sum = 0.0;
    let mut deep_sum = 0.0;
    let mut deep_peak = 0.0_f32;
    for n in 0..PITCH_WINDOW {
        let sample = (TWO_PI * 155.0 * n as f32 / DEFAULT_SAMPLE_RATE as f32).sin() * 0.24;
        bright_sum += bright_processor.process_sample(sample, &bright).abs();
        let deep_sample = deep_processor.process_sample(sample, &deep);
        deep_sum += deep_sample.abs();
        deep_peak = deep_peak.max(deep_sample.abs());
    }

    assert!(deep_peak > 0.02);
    assert!((bright_sum - deep_sum).abs() > 0.01);
}

#[test]
fn cinematic_high_is_distinct_and_stays_voiced() {
    let deep = SharedDspParams::new();
    deep.set_robot_amount(0.9);
    deep.set_monotone(true);
    deep.set_voice_mode(VoiceMode::DeepMorph);

    let cinematic = SharedDspParams::new();
    cinematic.set_robot_amount(0.98);
    cinematic.set_monotone(true);
    cinematic.set_voice_mode(VoiceMode::CinematicHigh);

    let mut deep_processor = seeded_processor();
    let mut cinematic_processor = seeded_processor();
    let mut deep_sum = 0.0;
    let mut cinematic_sum = 0.0;
    let mut cinematic_peak = 0.0_f32;
    for n in 0..PITCH_WINDOW {
        let sample = (TWO_PI * 145.0 * n as f32 / DEFAULT_SAMPLE_RATE as f32).sin() * 0.22;
        deep_sum += deep_processor.process_sample(sample, &deep).abs();
        let cinematic_sample = cinematic_processor.process_sample(sample, &cinematic);
        cinematic_sum += cinematic_sample.abs();
        cinematic_peak = cinematic_peak.max(cinematic_sample.abs());
    }

    assert!(cinematic_peak > 0.02);
    assert!((deep_sum - cinematic_sum).abs() > 0.01);
}

#[test]
fn synthetic_consonant_noise_is_smoothed() {
    let mut processor = seeded_processor();
    let mut previous = processor.next_soft_noise();
    let mut max_delta = 0.0_f32;
    for _ in 0..512 {
        let current = processor.next_soft_noise();
        max_delta = max_delta.max((current - previous).abs());
        previous = current;
    }

    assert!(max_delta < 0.30);
}

#[test]
fn monotone_keeps_signal_voiced_without_buzz_carrier() {
    let params = SharedDspParams::new();
    params.set_robot_amount(0.7);
    params.set_monotone(true);

    let mut processor = VoiceProcessor::default();
    let mut max = 0.0_f32;
    for n in 0..PITCH_WINDOW {
        let sample = (TWO_PI * 180.0 * n as f32 / DEFAULT_SAMPLE_RATE as f32).sin() * 0.25;
        max = max.max(processor.process_sample(sample, &params).abs());
    }

    assert!(max > 0.02);
    assert!(max < 1.0);
}

#[test]
fn delay_reader_wraps_exact_buffer_boundary() {
    let mut processor = VoiceProcessor::default();
    processor.pitch_buffer[0] = 0.25;
    processor.pitch_buffer[1] = 0.5;

    assert_eq!(processor.read_delay(PITCH_BUFFER as f32), 0.25);
}

fn seeded_processor() -> VoiceProcessor {
    let mut processor = VoiceProcessor::default();
    processor.jitter_seed = 0x1234_5678;
    processor.noise_seed = 0x8765_4321;
    processor
}
