use anyhow::{Context, Result};
use atomic_float::AtomicF32;
use pipewire as pw;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    time::Duration,
};

use crate::dsp::SharedDspParams;

use super::{
    engine::AudioEvent,
    format::MAX_BUFFERED_SAMPLES,
    sample_queue::SampleQueue,
    streams::{create_capture_stream, create_monitor_stream, create_virtual_source_stream},
};

pub(super) fn run_pipewire(
    input_node_id: u32,
    dsp_params: Arc<SharedDspParams>,
    event_sender: Sender<AudioEvent>,
    stop: Arc<AtomicBool>,
    level: Arc<AtomicF32>,
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
        &level,
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
            Some(Duration::from_millis(50)),
            Some(Duration::from_millis(50)),
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
