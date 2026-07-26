mod capture;
mod output;

use pipewire as pw;

use super::format::{CHANNELS, REQUESTED_QUANTUM};

pub(super) use capture::create_capture_stream;
pub(super) use output::{create_monitor_stream, create_virtual_source_stream};

fn insert_audio_props(stream_props: &mut pw::properties::Properties) {
    let rate = crate::dsp::DEFAULT_SAMPLE_RATE;
    stream_props.insert("audio.rate", rate.to_string());
    stream_props.insert("audio.channels", CHANNELS.to_string());
    stream_props.insert("audio.format", "F32LE");
    // Without these the graph is free to run our nodes at its default quantum,
    // which is 1024 samples on most desktops and adds about 21 ms per hop. The
    // capture and the playback hop both pay it, so asking for a small quantum
    // is the single largest end to end latency win outside the DSP itself.
    stream_props.insert("node.latency", format!("{REQUESTED_QUANTUM}/{rate}"));
    stream_props.insert("node.rate", format!("1/{rate}"));
}
