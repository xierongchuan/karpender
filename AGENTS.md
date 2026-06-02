# Karpender Agent Guide

## Project Overview

Karpender is a Rust GTK/libadwaita desktop app for Fedora/Linux that reads a physical microphone through PipeWire, applies voice anonymization DSP, exposes a virtual microphone, and optionally monitors the processed audio to speakers.

Core flow:

```text
physical mic -> PipeWire capture stream -> DSP -> virtual mic queue -> PipeWire virtual source
                                                -> optional monitor queue -> speakers
```

## Important Files

- `src/main.rs`: module entrypoint.
- `src/app.rs`: GTK/libadwaita application bootstrap.
- `src/ui/window.rs`: main window, controls, settings wiring, Start/Stop.
- `src/audio/devices.rs`: input source discovery via `pw-dump`.
- `src/audio/engine.rs`: PipeWire streams, queues, virtual mic, optional monitor stream.
- `src/dsp/mod.rs`: real-time-ish anonymizer DSP and tests.
- `src/config.rs`: JSON settings under XDG config.

## Commands

- Format: `cargo fmt`
- Format check: `cargo fmt --check`
- Compile check: `cargo check`
- Tests: `cargo test`
- Run app locally: `cargo run`

PipeWire inspection helpers:

- List nodes: `pw-dump --no-colors`
- Compact node view:

```bash
pw-dump --no-colors | jq -r '.[] | select(.type=="PipeWire:Interface:Node") | [.id, .info.props["media.class"], .info.props["node.name"], .info.props["node.description"], .info.state] | @tsv'
```

## Implementation Notes

- Keep audio callback code panic-free. PipeWire callbacks cannot unwind safely; panics can abort the whole process.
- Avoid allocations, blocking locks, and UI calls inside audio callbacks. Existing code uses `try_lock` and channel events.
- Do not let monitor playback consume from the virtual mic queue. It must use a separate queue so apps like Telegram still receive audio.
- Capture routing depends on `"target.object"` and WirePlumber/PipeWire autoconnect. Be careful when changing stream properties.
- DSP controls are currently named by behavior in the UI:
  - `Noise Cleanup`: soft downward expander, not a hard gate.
  - `Privacy Amount`: strength of voice disguise.
  - `Flatten Intonation`: reduces recognizable prosody without replacing speech with a carrier.
- Keep speech intelligibility as a first-class requirement. Prefer moderate pitch/formant/timbre changes over harsh robot/vocoder effects.

## Verification Expectations

For code changes, run at least:

```bash
cargo fmt --check
cargo check
cargo test
```

Manual audio checks should cover:

- selected physical mic raises the level meter;
- `Karpender Privacy Voice Mic` appears as an input source in Telegram/browser apps;
- monitor mode plays processed speech to speakers/headphones;
- Stop releases streams without hanging;
- no feedback loop when monitor mode is used with speakers near the mic.

Use headphones when testing monitor mode.
