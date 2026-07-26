"""Reference model of src/dsp/pitch.rs, used to pin down why the shifted voice
sounded detuned and metallic.

A delay line shifter reads its input through a delay that is periodic with the
grain length, so the output of a sine can only contain components at
``f_in + k / grain_period``. The target ``f_in * ratio`` is generally not on
that grid, so the shifter lands on the nearest grid line instead: a fixed grain
of 768 samples shifts 150 Hz by 1.5 to 212 Hz rather than 225 Hz, and every
harmonic of a voice picks up a different error, which is what turns the result
inharmonic and squeaky.

Making the grain a whole number of input periods puts a grid line exactly on the
target. Two periods are needed rather than one because the second tap sits half
a grain behind the first, and a half grain offset is only a whole number of
periods when the grain holds an even count of them. Otherwise the two taps are
in antiphase and cancel.

Run with: python3 experiments/pitch_shifter_spectrum.py
"""

import math

SR = 48000
BUFFER = 4096
BASE_DELAY = 24.0
MIN_GRAIN = 256.0
MAX_GRAIN = 1152.0


def shift(ratio, source_hz, grain, samples=16384):
    """Mirror of PitchShifter::process for a steady sine input."""
    buffer = [0.0] * BUFFER
    write = 0
    phase = 0.0
    out = []
    for index in range(samples):
        buffer[write] = math.sin(2 * math.pi * source_hz * index / SR) * 0.4
        write = (write + 1) % BUFFER

        def read(delay):
            position = (write - delay) % BUFFER
            low = int(math.floor(position))
            fraction = position - low
            first = buffer[low % BUFFER]
            second = buffer[(low + 1) % BUFFER]
            return first + (second - first) * fraction

        first = read(phase * grain + BASE_DELAY)
        second = read(((phase + 0.5) % 1.0) * grain + BASE_DELAY)
        fade = 0.5 * (1.0 - math.cos(2 * math.pi * phase))
        phase = (phase + (1.0 - ratio) / grain) % 1.0
        out.append(first * fade + second * (1.0 - fade))

    return out[samples // 2:]


def dominant_hz(samples, low, high):
    """Strongest bin in a range, Hann windowed to keep leakage out of it."""
    count = len(samples)
    best = (0.0, low)
    for frequency in range(low, high + 1):
        real = imaginary = 0.0
        for index, value in enumerate(samples):
            weight = 0.5 - 0.5 * math.cos(2 * math.pi * index / count)
            angle = 2 * math.pi * frequency * index / SR
            real += value * weight * math.cos(angle)
            imaginary += value * weight * math.sin(angle)
        magnitude = math.hypot(real, imaginary) / count
        if magnitude > best[0]:
            best = (magnitude, frequency)
    return best[1]


def grain_window(period):
    """Same rule as dsp::pitch::grain_window."""
    return min(max(2.0 * period, MIN_GRAIN), MAX_GRAIN)


CASES = [(150.0, 1.5), (200.0, 0.75), (110.0, 1.35), (240.0, 0.8), (175.0, 1.2), (95.0, 1.4)]

print(f"{'input':>8} {'ratio':>6} {'target':>8} {'fixed grain':>12} {'synced grain':>13}")
for source_hz, ratio in CASES:
    period = SR / source_hz
    grain = grain_window(period)
    target = source_hz * ratio
    span = (int(target) - 45, int(target) + 45)
    fixed = dominant_hz(shift(ratio, source_hz, MAX_GRAIN), *span)
    synced = dominant_hz(shift(ratio, source_hz, grain), *span)
    print(f"{source_hz:8.1f} {ratio:6.2f} {target:8.1f} {fixed:12d} {synced:13d}")
