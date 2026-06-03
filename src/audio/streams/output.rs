use anyhow::{Context, Result};
use pipewire as pw;
use pw::{properties::properties, spa};
use std::sync::{Arc, mpsc::Sender};

use super::insert_audio_props;
use crate::audio::{
    engine::AudioEvent,
    format::{SAMPLE_SIZE, audio_info, audio_params},
    sample_queue::SampleQueue,
};

pub(in crate::audio) fn create_monitor_stream<'c>(
    core: &'c pw::core::Core,
    sample_queue: &Arc<SampleQueue>,
    event_sender: &Sender<AudioEvent>,
) -> Result<RegisteredMonitorStream<'c>> {
    let mut stream_props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Playback",
        *pw::keys::MEDIA_ROLE => "Communication",
        *pw::keys::NODE_NAME => "KarpenderPrivacyVoiceMonitor",
        *pw::keys::NODE_DESCRIPTION => "Karpender Privacy Voice Monitor",
    };
    insert_audio_props(&mut stream_props);

    let stream = pw::stream::StreamBox::new(core, "Karpender Privacy Voice Monitor", stream_props)
        .context("failed to create monitor playback stream")?;

    let queue = Arc::clone(sample_queue);
    let state_events = event_sender.clone();
    let listener = stream
        .add_local_listener_with_user_data(())
        .state_changed(move |_, _, _, new| {
            if let pw::stream::StreamState::Error(error) = new {
                let _ = state_events.send(AudioEvent::Error(format!(
                    "monitor playback stream failed: {error}"
                )));
            }
        })
        .process(move |stream, _| fill_output_buffer(stream, &queue))
        .register()
        .context("failed to register monitor playback listener")?;

    let mut params = audio_params(audio_info())?;

    stream
        .connect(
            spa::utils::Direction::Output,
            None,
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS,
            &mut params,
        )
        .context("failed to connect monitor playback stream")?;

    Ok(RegisteredMonitorStream {
        _stream: stream,
        _listener: listener,
    })
}

pub(in crate::audio) fn create_virtual_source_stream<'c>(
    core: &'c pw::core::Core,
    sample_queue: &Arc<SampleQueue>,
    event_sender: &Sender<AudioEvent>,
) -> Result<RegisteredSourceStream<'c>> {
    let mut stream_props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Communication",
        *pw::keys::MEDIA_CLASS => "Audio/Source",
        *pw::keys::NODE_NAME => "KarpenderPrivacyVoiceMic",
        *pw::keys::NODE_DESCRIPTION => "Karpender Privacy Voice Mic",
        *pw::keys::NODE_AUTOCONNECT => "false",
    };
    insert_audio_props(&mut stream_props);

    let stream = pw::stream::StreamBox::new(core, "Karpender Privacy Voice Mic", stream_props)
        .context("failed to create virtual microphone stream")?;

    let queue = Arc::clone(sample_queue);
    let state_events = event_sender.clone();
    let listener = stream
        .add_local_listener_with_user_data(())
        .state_changed(move |_, _, _, new| {
            if let pw::stream::StreamState::Error(error) = new {
                let _ = state_events.send(AudioEvent::Error(format!(
                    "virtual microphone stream failed: {error}"
                )));
            }
        })
        .process(move |stream, _| fill_output_buffer(stream, &queue))
        .register()
        .context("failed to register virtual microphone listener")?;

    let mut params = audio_params(audio_info())?;

    stream
        .connect(
            spa::utils::Direction::Output,
            None,
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS,
            &mut params,
        )
        .context("failed to connect virtual microphone stream")?;

    Ok(RegisteredSourceStream {
        _stream: stream,
        _listener: listener,
    })
}

fn fill_output_buffer(stream: &pw::stream::Stream, sample_queue: &Arc<SampleQueue>) {
    let Some(mut buffer) = stream.dequeue_buffer() else {
        return;
    };
    let datas = buffer.datas_mut();
    if datas.is_empty() {
        return;
    }

    let data = &mut datas[0];
    let Some(slice) = data.data() else {
        return;
    };

    let n_frames = sample_queue.fill_bytes(slice);

    let chunk = data.chunk_mut();
    *chunk.offset_mut() = 0;
    *chunk.stride_mut() = SAMPLE_SIZE as _;
    *chunk.size_mut() = (n_frames * SAMPLE_SIZE) as _;
}

pub(in crate::audio) struct RegisteredSourceStream<'c> {
    _stream: pw::stream::StreamBox<'c>,
    _listener: pw::stream::StreamListener<()>,
}

pub(in crate::audio) struct RegisteredMonitorStream<'c> {
    _stream: pw::stream::StreamBox<'c>,
    _listener: pw::stream::StreamListener<()>,
}
