# Karpender

Karpender is a Fedora/Linux desktop app for voice privacy. It reads a physical microphone through PipeWire, applies voice anonymization DSP, and exposes the result as a virtual microphone for apps such as Telegram, Discord, OBS, browsers, and Zoom.

```text
physical mic -> Karpender DSP -> Karpender Privacy Voice Mic -> chat/recording app
```

## Features

- GTK4/libadwaita desktop UI.
- PipeWire input source selection.
- Virtual microphone output.
- Voice anonymization with pitch/timbre/prosody changes.
- Eight voice modes, each with its own pitch target and head size.
- Noise Cleanup as a soft expander, not a hard gate.
- Privacy Amount for stronger or lighter voice disguise.
- Flatten Intonation mode for reducing recognizable prosody.
- Monitor to Speakers mode for hearing the processed voice locally.

Use headphones when testing monitor mode to avoid feedback.

## Voice Modes

Every mode sets its own target pitch and its own formant shift, so two modes at
the same pitch still sound like two different people. The mode picker shows the
same hint as a tooltip.

| Mode | Character |
| --- | --- |
| Masked Voice | Neutral mid pitch, unchanged formants |
| Bright Stranger | Higher pitch with a brighter, smaller head |
| Deep Morph | Low pitch with a larger, darker head |
| Cinematic High | High pitch, wide open and airy |
| Warm Neighbour | Low mid pitch, warm and close |
| Calm Androgynous | Mid pitch, deliberately gender neutral |
| Soft Alto | High mid pitch, soft and breathy |
| Low Baritone | Lowest pitch, full chest resonance |

On top of the mode, every run of the app draws a private random identity: a
small extra pitch offset, a formant offset, a spectral tilt and a per band
colour. The same speaker in the same mode therefore does not produce the same
voice twice, which is what makes a recording harder to match against another
one.

## Latency

The processing path is built to stay inside a normal conversation:

| Stage | Cost at 48 kHz |
| --- | --- |
| PipeWire quantum, requested by both streams | 256 samples, 5.3 ms per hop |
| Pitch shifter grain, two periods of the tracked pitch | 664 samples at 150 Hz, 13.8 ms |
| Queue cushion between capture and playback | 64 to 512 samples |

The grain follows the voice, so a higher voice pays less. A deep voice at 83 Hz
reaches the ceiling of 1176 samples, 24.5 ms.

## Requirements

Runtime:

- Fedora or another PipeWire-based Linux desktop.
- PipeWire and WirePlumber running.
- GTK4 and libadwaita runtime libraries.
- `pw-dump` available in `PATH` for input device discovery.

Development packages on Fedora:

```bash
sudo dnf install rust cargo gtk4-devel libadwaita-devel pipewire-devel clang pkgconf-pkg-config jq
```

## Run From Source

```bash
cargo run
```

In Karpender:

1. Select the real microphone in `Input Device`.
2. Adjust `Privacy Amount`, `Noise Cleanup`, and `Flatten Intonation`.
3. Press `Start Processing`.
4. In Telegram/Discord/browser, select `Karpender Privacy Voice Mic` as the microphone.

## AppImage Build

The AppImage setup lives in:

- `scripts/build-appimage.sh`
- `packaging/appimage/AppRun`
- `data/com.github.xierongchuan.karpender.desktop`
- `data/com.github.xierongchuan.karpender.metainfo.xml`
- `data/icons/com.github.xierongchuan.karpender.svg`

Recommended tooling in `PATH`:

- `linuxdeploy`
- `linuxdeploy-plugin-gtk`

Fallback:

- `appimagetool`

Build:

```bash
scripts/build-appimage.sh
```

This command only builds local artifacts in `target/appimage` and `dist`. It does not install Karpender into the system.
If neither `linuxdeploy` nor `appimagetool` is found in `PATH`, the script downloads a local portable `appimagetool` into `target/appimage-tools` and uses it from there.

Output:

```text
dist/Karpender-<version>-<arch>.AppImage
```

If no AppImage builder can be found or downloaded, the script still prepares the AppDir at:

```text
target/appimage/Karpender.AppDir
```

Then put `appimagetool` in `PATH` or at `target/appimage-tools/appimagetool-<arch>.AppImage` and rerun the script.

## Flatpak Build

The Flatpak setup lives in:

- `scripts/build-flatpak.sh`
- `scripts/generate-flatpak-cargo-sources.py`
- `packaging/flatpak/com.github.xierongchuan.karpender.json`
- `packaging/flatpak/cargo-sources.json`

Required tooling:

- `flatpak`
- `flatpak-builder`
- Flathub configured for the user Flatpak installation. `scripts/build-flatpak.sh` adds the user remote automatically if it is missing.

Build a local bundle:

```bash
scripts/build-flatpak.sh
```

The script regenerates `packaging/flatpak/cargo-sources.json` from `Cargo.lock`, builds against `org.gnome.Platform//50` and the Rust/LLVM SDK extensions, writes a local OSTree repository under `target/flatpak`, and exports:

```text
dist/Karpender-<version>.flatpak
```

Install the exported bundle locally:

```bash
flatpak install --user dist/Karpender-<version>.flatpak
flatpak run com.github.xierongchuan.karpender
```

The manifest grants Wayland/X11 fallback, GPU, PulseAudio, and PipeWire socket access so Karpender can show the GTK UI, read microphone nodes, create the virtual microphone, and monitor processed audio.

## Development Checks

```bash
cargo fmt --check
cargo check
cargo test
```

## PipeWire Debugging

List PipeWire nodes:

```bash
pw-dump --no-colors
```

Compact node view:

```bash
pw-dump --no-colors | jq -r '.[] | select(.type=="PipeWire:Interface:Node") | [.id, .info.props["media.class"], .info.props["node.name"], .info.props["node.description"], .info.state] | @tsv'
```

Expected flow while running:

- real microphone node is selected by Karpender;
- `Karpender Privacy Voice Mic` appears as an `Audio/Source`;
- Telegram/Discord/browser links to the virtual source.

## Notes

Karpender aims to make voice recognition harder while keeping speech understandable. It is not a guarantee of anonymity against forensic voice analysis.
