use anyhow::{Context, Result};
use pipewire as pw;
use pw::spa;
use spa::pod::Pod;
use std::mem;

use crate::dsp::DEFAULT_SAMPLE_RATE;

pub(super) const CHANNELS: u32 = 1;
pub(super) const SAMPLE_SIZE: usize = mem::size_of::<f32>();
/// Ring size for the queues between capture and playback. It is a hard ceiling
/// on how far the reader can fall behind, so it doubles as a latency ceiling:
/// 100 ms is already generous next to the quantum we ask PipeWire for.
pub(super) const MAX_BUFFERED_SAMPLES: usize = DEFAULT_SAMPLE_RATE as usize / 10;

/// Quantum requested from the graph, about 5.3 ms at 48 kHz. PipeWire treats
/// this as a hint and still honours the driver, but asking keeps the graph from
/// settling on the default 1024 sample quantum when nothing else needs it.
pub(super) const REQUESTED_QUANTUM: u32 = 256;

pub(super) fn audio_info() -> spa::param::audio::AudioInfoRaw {
    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::F32LE);
    audio_info.set_rate(DEFAULT_SAMPLE_RATE);
    audio_info.set_channels(CHANNELS);
    audio_info
}

pub(super) fn with_audio_params<T>(
    audio_info: spa::param::audio::AudioInfoRaw,
    connect: impl FnOnce(&mut [&Pod; 1]) -> Result<T>,
) -> Result<T> {
    let obj = pw::spa::pod::Object {
        type_: pw::spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: pw::spa::param::ParamType::EnumFormat.as_raw(),
        properties: audio_info.into(),
    };
    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(obj),
    )
    .context("failed to serialize audio format")?
    .0
    .into_inner();

    let pod = Pod::from_bytes(&values)
        .ok_or_else(|| anyhow::anyhow!("failed to build PipeWire audio format pod"))?;
    let mut params = [pod];

    connect(&mut params)
}

pub(super) fn f32_from_le_slice(bytes: &[u8]) -> f32 {
    let Ok(sample) = bytes.try_into() else {
        return 0.0;
    };

    f32::from_le_bytes(sample)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_sample_slice_decodes_as_silence() {
        assert_eq!(f32_from_le_slice(&[1, 2]), 0.0);
    }
}
