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
- Noise Cleanup as a soft expander, not a hard gate.
- Privacy Amount for stronger or lighter voice disguise.
- Flatten Intonation mode for reducing recognizable prosody.
- Monitor to Speakers mode for hearing the processed voice locally.

Use headphones when testing monitor mode to avoid feedback.

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

Karpender is an MVP. It aims to make voice recognition harder while keeping speech understandable. It is not a guarantee of anonymity against forensic voice analysis.
