use anyhow::{Context, Result};
use pipewire as pw;
use pw::{properties::properties, spa};
use spa::pod::Pod;
use std::{
    collections::VecDeque,
    mem,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    thread::{self, JoinHandle},
};

use crate::dsp::{DEFAULT_SAMPLE_RATE, SharedDspParams, VoiceProcessor};

const CHANNELS: u32 = 1;
const SAMPLE_SIZE: usize = mem::size_of::<f32>();
const MAX_BUFFERED_SAMPLES: usize = DEFAULT_SAMPLE_RATE as usize;

#[derive(Debug, Clone)]
pub enum AudioEvent {
    Level(f32),
    Started,
    Stopped,
    Error(String),
}

#[derive(Debug)]
pub struct AudioEngine {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl AudioEngine {
    pub fn start(
        input_node_id: u32,
        dsp_params: Arc<SharedDspParams>,
        event_sender: Sender<AudioEvent>,
        monitor_output: bool,
    ) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let handle = thread::Builder::new()
            .name("privacy-voice-pipewire".to_string())
            .spawn(move || {
                let result = run_pipewire(
                    input_node_id,
                    dsp_params,
                    event_sender.clone(),
                    thread_stop,
                    monitor_output,
                );
                if let Err(error) = result {
                    let _ = event_sender.send(AudioEvent::Error(error.to_string()));
                }
            })
            .context("failed to spawn PipeWire thread")?;

        Ok(Self {
            stop,
            handle: Some(handle),
        })
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_pipewire(
    input_node_id: u32,
    dsp_params: Arc<SharedDspParams>,
    event_sender: Sender<AudioEvent>,
    stop: Arc<AtomicBool>,
    monitor_output: bool,
) -> Result<()> {
    pw::init();

    let mainloop =
        pw::main_loop::MainLoopRc::new(None).context("failed to create PipeWire loop")?;
    let context = pw::context::ContextRc::new(&mainloop, None)
        .context("failed to create PipeWire context")?;
    let core = context
        .connect_rc(None)
        .context("failed to connect to PipeWire")?;
    let virtual_queue = Arc::new(SampleQueue::with_capacity(4096, MAX_BUFFERED_SAMPLES));
    let monitor_queue =
        monitor_output.then(|| Arc::new(SampleQueue::with_capacity(4096, MAX_BUFFERED_SAMPLES)));

    let capture_stream = create_capture_stream(
        &core,
        input_node_id,
        &virtual_queue,
        monitor_queue.as_ref(),
        &dsp_params,
        &event_sender,
    )?;
    let source_stream = create_virtual_source_stream(&core, &virtual_queue, &event_sender)?;
    let monitor_stream = if let Some(queue) = &monitor_queue {
        Some(create_monitor_stream(&core, queue, &event_sender)?)
    } else {
        None
    };

    let timer_sender = event_sender.clone();
    let stop_for_timer = Arc::clone(&stop);
    let mainloop_for_timer = mainloop.clone();
    let _timer = mainloop.loop_().add_timer(move |_| {
        if stop_for_timer.load(Ordering::Acquire) {
            let _ = timer_sender.send(AudioEvent::Stopped);
            mainloop_for_timer.quit();
        }
    });
    _timer
        .update_timer(
            Some(std::time::Duration::from_millis(50)),
            Some(std::time::Duration::from_millis(50)),
        )
        .into_result()
        .context("failed to arm PipeWire stop timer")?;

    let _ = event_sender.send(AudioEvent::Started);
    mainloop.run();

    drop(monitor_stream);
    drop(source_stream);
    drop(capture_stream);

    Ok(())
}

fn create_capture_stream<'c>(
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
    stream_props.insert("audio.rate", DEFAULT_SAMPLE_RATE.to_string());
    stream_props.insert("audio.channels", CHANNELS.to_string());
    stream_props.insert("audio.format", "F32LE");

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

                let processed = processor.process_sample(mono, &params);
                peak = peak.max(processed.abs());

                virtual_queue.push_sample(processed);
                if let Some(monitor_queue) = &monitor_queue {
                    monitor_queue.push_sample(processed);
                }
            }

            let _ = events.send(AudioEvent::Level(peak));
        })
        .register()
        .context("failed to register capture listener")?;

    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::F32LE);
    audio_info.set_rate(DEFAULT_SAMPLE_RATE);
    audio_info.set_channels(CHANNELS);
    let mut params = audio_params(audio_info)?;

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

fn create_monitor_stream<'c>(
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
    stream_props.insert("audio.rate", DEFAULT_SAMPLE_RATE.to_string());
    stream_props.insert("audio.channels", CHANNELS.to_string());
    stream_props.insert("audio.format", "F32LE");

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

    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::F32LE);
    audio_info.set_rate(DEFAULT_SAMPLE_RATE);
    audio_info.set_channels(CHANNELS);
    let mut params = audio_params(audio_info)?;

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

fn create_virtual_source_stream<'c>(
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
    stream_props.insert("audio.rate", DEFAULT_SAMPLE_RATE.to_string());
    stream_props.insert("audio.channels", CHANNELS.to_string());
    stream_props.insert("audio.format", "F32LE");

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

    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::F32LE);
    audio_info.set_rate(DEFAULT_SAMPLE_RATE);
    audio_info.set_channels(CHANNELS);
    let mut params = audio_params(audio_info)?;

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

#[derive(Debug)]
struct SampleQueue {
    samples: Mutex<VecDeque<f32>>,
    max_samples: usize,
}

impl SampleQueue {
    fn with_capacity(capacity: usize, max_samples: usize) -> Self {
        Self {
            samples: Mutex::new(VecDeque::with_capacity(capacity)),
            max_samples,
        }
    }

    fn push_sample(&self, sample: f32) {
        let Ok(mut samples) = self.samples.try_lock() else {
            return;
        };

        if samples.len() >= self.max_samples {
            samples.pop_front();
        }
        samples.push_back(sample);
    }

    fn fill_bytes(&self, output: &mut [u8]) -> usize {
        let n_frames = output.len() / SAMPLE_SIZE;
        let Ok(mut samples) = self.samples.try_lock() else {
            output.fill(0);
            return n_frames;
        };

        for frame in output.chunks_exact_mut(SAMPLE_SIZE) {
            let sample = samples.pop_front().unwrap_or_default();
            frame.copy_from_slice(&sample.to_le_bytes());
        }

        n_frames
    }
}

fn f32_from_le_slice(bytes: &[u8]) -> f32 {
    let Ok(sample) = bytes.try_into() else {
        return 0.0;
    };

    f32::from_le_bytes(sample)
}

fn audio_params(audio_info: spa::param::audio::AudioInfoRaw) -> Result<[&'static Pod; 1]> {
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

    let pod = Pod::from_bytes(Box::leak(values.into_boxed_slice()))
        .ok_or_else(|| anyhow::anyhow!("failed to build PipeWire audio format pod"))?;

    Ok([pod])
}

#[derive(Debug, Default)]
struct CaptureData {
    format: spa::param::audio::AudioInfoRaw,
    processor: VoiceProcessor,
}

struct RegisteredCaptureStream<'c> {
    _stream: pw::stream::StreamBox<'c>,
    _listener: pw::stream::StreamListener<CaptureData>,
}

struct RegisteredSourceStream<'c> {
    _stream: pw::stream::StreamBox<'c>,
    _listener: pw::stream::StreamListener<()>,
}

struct RegisteredMonitorStream<'c> {
    _stream: pw::stream::StreamBox<'c>,
    _listener: pw::stream::StreamListener<()>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_queue_drops_oldest_samples_when_full() {
        let queue = SampleQueue::with_capacity(2, 2);
        queue.push_sample(0.25);
        queue.push_sample(0.5);
        queue.push_sample(0.75);

        let mut output = vec![0; SAMPLE_SIZE * 2];
        assert_eq!(queue.fill_bytes(&mut output), 2);

        let first = f32_from_le_slice(&output[..SAMPLE_SIZE]);
        let second = f32_from_le_slice(&output[SAMPLE_SIZE..]);

        assert_eq!(first, 0.5);
        assert_eq!(second, 0.75);
    }

    #[test]
    fn sample_queue_zero_fills_when_empty() {
        let queue = SampleQueue::with_capacity(2, 2);
        let mut output = vec![255; SAMPLE_SIZE * 2];

        assert_eq!(queue.fill_bytes(&mut output), 2);

        assert!(output.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn short_sample_slice_decodes_as_silence() {
        assert_eq!(f32_from_le_slice(&[1, 2]), 0.0);
    }
}
