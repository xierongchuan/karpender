mod capture;
mod output;

use pipewire as pw;

use super::format::CHANNELS;

pub(super) use capture::create_capture_stream;
pub(super) use output::{create_monitor_stream, create_virtual_source_stream};

fn insert_audio_props(stream_props: &mut pw::properties::Properties) {
    stream_props.insert("audio.rate", crate::dsp::DEFAULT_SAMPLE_RATE.to_string());
    stream_props.insert("audio.channels", CHANNELS.to_string());
    stream_props.insert("audio.format", "F32LE");
}
