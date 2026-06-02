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
    ) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let handle = thread::Builder::new()
            .name("privacy-voice-pipewire".to_string())
            .spawn(move || {
                let result =
                    run_pipewire(input_node_id, dsp_params, event_sender.clone(), thread_stop);
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
) -> Result<()> {
    pw::init();

    let mainloop =
        pw::main_loop::MainLoopRc::new(None).context("failed to create PipeWire loop")?;
    let context = pw::context::ContextRc::new(&mainloop, None)
        .context("failed to create PipeWire context")?;
    let core = context
        .connect_rc(None)
        .context("failed to connect to PipeWire")?;
    let sample_queue = Arc::new(Mutex::new(VecDeque::<f32>::with_capacity(4096)));

    let capture_stream = create_capture_stream(
        &core,
        input_node_id,
        &sample_queue,
        &dsp_params,
        &event_sender,
    )?;
    let source_stream = create_virtual_source_stream(&core, &sample_queue)?;

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

    drop(source_stream);
    drop(capture_stream);

    Ok(())
}

fn create_capture_stream<'c>(
    core: &'c pw::core::Core,
    input_node_id: u32,
    sample_queue: &Arc<Mutex<VecDeque<f32>>>,
    dsp_params: &Arc<SharedDspParams>,
    event_sender: &Sender<AudioEvent>,
) -> Result<RegisteredCaptureStream<'c>> {
    let stream = pw::stream::StreamBox::new(
        core,
        "privacy-voice-capture",
        properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Communication",
            *pw::keys::NODE_NAME => "PrivacyVoiceCapture",
        },
    )
    .context("failed to create capture stream")?;

    let queue = Arc::clone(sample_queue);
    let params = Arc::clone(dsp_params);
    let events = event_sender.clone();
    let listener = stream
        .add_local_listener_with_user_data(CaptureData::default())
        .param_changed(|_, user_data, id, param| {
            if let Some(param) = param {
                if id == pw::spa::param::ParamType::Format.as_raw() {
                    let _ = user_data.format.parse(param);
                }
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
            let n_samples = data.chunk().size() as usize / SAMPLE_SIZE;

            let Some(samples) = data.data() else {
                return;
            };

            let processor = &mut user_data.processor;
            let mut peak = 0.0_f32;
            if let Ok(mut queue) = queue.try_lock() {
                for frame in
                    samples[..n_samples * SAMPLE_SIZE].chunks_exact(SAMPLE_SIZE * n_channels)
                {
                    let mut mono = 0.0_f32;
                    for channel in 0..n_channels {
                        let start = channel * SAMPLE_SIZE;
                        let bytes = &frame[start..start + SAMPLE_SIZE];
                        mono += f32::from_le_bytes(bytes.try_into().unwrap_or([0; SAMPLE_SIZE]));
                    }
                    mono /= n_channels as f32;

                    let processed = processor.process_sample(mono, &params);
                    peak = peak.max(processed.abs());

                    if queue.len() >= MAX_BUFFERED_SAMPLES {
                        queue.pop_front();
                    }
                    queue.push_back(processed);
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
            Some(input_node_id),
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

fn create_virtual_source_stream<'c>(
    core: &'c pw::core::Core,
    sample_queue: &Arc<Mutex<VecDeque<f32>>>,
) -> Result<RegisteredSourceStream<'c>> {
    let stream = pw::stream::StreamBox::new(
        core,
        "Privacy Voice Mic",
        properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Communication",
            *pw::keys::MEDIA_CLASS => "Audio/Source",
            *pw::keys::NODE_NAME => "PrivacyVoiceMic",
            *pw::keys::NODE_DESCRIPTION => "Privacy Voice Mic",
            *pw::keys::NODE_AUTOCONNECT => "false",
        },
    )
    .context("failed to create virtual microphone stream")?;

    let queue = Arc::clone(sample_queue);
    let listener = stream
        .add_local_listener_with_user_data(())
        .process(move |stream, _| {
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

            let n_frames = slice.len() / SAMPLE_SIZE;
            if let Ok(mut queue) = queue.try_lock() {
                for frame_index in 0..n_frames {
                    let sample = queue.pop_front().unwrap_or(0.0);
                    let start = frame_index * SAMPLE_SIZE;
                    slice[start..start + SAMPLE_SIZE].copy_from_slice(&sample.to_le_bytes());
                }
            } else {
                slice.fill(0);
            }

            let chunk = data.chunk_mut();
            *chunk.offset_mut() = 0;
            *chunk.stride_mut() = SAMPLE_SIZE as _;
            *chunk.size_mut() = (n_frames * SAMPLE_SIZE) as _;
        })
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

#[derive(Debug)]
struct CaptureData {
    format: spa::param::audio::AudioInfoRaw,
    processor: VoiceProcessor,
}

impl Default for CaptureData {
    fn default() -> Self {
        Self {
            format: Default::default(),
            processor: VoiceProcessor::default(),
        }
    }
}

struct RegisteredCaptureStream<'c> {
    _stream: pw::stream::StreamBox<'c>,
    _listener: pw::stream::StreamListener<CaptureData>,
}

struct RegisteredSourceStream<'c> {
    _stream: pw::stream::StreamBox<'c>,
    _listener: pw::stream::StreamListener<()>,
}
