use anyhow::{Context, Result};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    thread::{self, JoinHandle},
};

use crate::dsp::SharedDspParams;

use super::session::run_pipewire;

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
