use anyhow::{Context, Result};
use pipewire as pw;
use pw::{properties::properties, spa};
use std::sync::{Arc, mpsc::Sender};

use crate::dsp::{SharedDspParams, VoiceProcessor};

use super::insert_audio_props;
use crate::audio::{
    engine::AudioEvent,
    format::{SAMPLE_SIZE, audio_info, audio_params, f32_from_le_slice},
    sample_queue::SampleQueue,
};

pub(in crate::audio) fn create_capture_stream<'c>(
    core: &'c pw::core::Core,
    input_node_id: u32,
    virtual_queue: &Arc<SampleQueue>,
    monitor_queue: Option<&Arc<SampleQueue>>,
    dsp_params: &Arc<SharedDspParams>,
    event_sender: &Sender<AudioEvent>,
) -> Result<RegisteredCaptureStream<'c>> {
    let mut stream_props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Communication",
        *pw::keys::NODE_NAME => "PrivacyVoiceCapture",
        *pw::keys::NODE_DESCRIPTION => "Privacy Voice Capture",
        "target.object" => input_node_id.to_string(),
    };
    insert_audio_props(&mut stream_props);

    let stream = pw::stream::StreamBox::new(core, "privacy-voice-capture", stream_props)
        .context("failed to create capture stream")?;

    let virtual_queue = Arc::clone(virtual_queue);
    let monitor_queue = monitor_queue.map(Arc::clone);
    let params = Arc::clone(dsp_params);
    let events = event_sender.clone();
    let state_events = event_sender.clone();
    let listener = stream
        .add_local_listener_with_user_data(CaptureData::default())
        .state_changed(move |_, _, _, new| {
            if let pw::stream::StreamState::Error(error) = new {
                let _ =
                    state_events.send(AudioEvent::Error(format!("capture stream failed: {error}")));
            }
        })
        .param_changed(|_, user_data, id, param| {
            if let Some(param) = param
                && id == pw::spa::param::ParamType::Format.as_raw()
            {
                let _ = user_data.format.parse(param);
            }
        })
        .process(move |stream, user_data| {
            process_capture_buffer(
                stream,
                user_data,
                &params,
                &virtual_queue,
                monitor_queue.as_ref(),
                &events,
            );
        })
        .register()
        .context("failed to register capture listener")?;

    let mut params = audio_params(audio_info())?;

    stream
        .connect(
            spa::utils::Direction::Input,
            None,
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS,
            &mut params,
        )
        .context("failed to connect capture stream")?;

    Ok(RegisteredCaptureStream {
        _stream: stream,
        _listener: listener,
    })
}

fn process_capture_buffer(
    stream: &pw::stream::Stream,
    user_data: &mut CaptureData,
    params: &Arc<SharedDspParams>,
    virtual_queue: &Arc<SampleQueue>,
    monitor_queue: Option<&Arc<SampleQueue>>,
    events: &Sender<AudioEvent>,
) {
    let Some(mut buffer) = stream.dequeue_buffer() else {
        return;
    };
    let datas = buffer.datas_mut();
    if datas.is_empty() {
        return;
    }

    let data = &mut datas[0];
    let n_channels = user_data.format.channels().max(1) as usize;
    let chunk_offset = data.chunk().offset() as usize;
    let chunk_bytes = data.chunk().size() as usize;

    let Some(samples) = data.data() else {
        return;
    };

    let frame_bytes = SAMPLE_SIZE * n_channels;
    if frame_bytes == 0 || chunk_offset >= samples.len() {
        return;
    }

    let available_bytes = if chunk_bytes == 0 {
        samples.len() - chunk_offset
    } else {
        chunk_bytes.min(samples.len() - chunk_offset)
    };
    let readable = &samples[chunk_offset..chunk_offset + available_bytes];

    let processor = &mut user_data.processor;
    let mut peak = 0.0_f32;
    for frame in readable.chunks_exact(frame_bytes) {
        let mut mono = 0.0_f32;
        for channel in 0..n_channels {
            let start = channel * SAMPLE_SIZE;
            let bytes = &frame[start..start + SAMPLE_SIZE];
            mono += f32_from_le_slice(bytes);
        }
        mono /= n_channels as f32;

        let processed = processor.process_sample(mono, params);
        peak = peak.max(processed.abs());

        virtual_queue.push_sample(processed);
        if let Some(monitor_queue) = monitor_queue {
            monitor_queue.push_sample(processed);
        }
    }

    let _ = events.send(AudioEvent::Level(peak));
}

#[derive(Debug, Default)]
struct CaptureData {
    format: spa::param::audio::AudioInfoRaw,
    processor: VoiceProcessor,
}

pub(in crate::audio) struct RegisteredCaptureStream<'c> {
    _stream: pw::stream::StreamBox<'c>,
    _listener: pw::stream::StreamListener<CaptureData>,
}
