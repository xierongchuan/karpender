use anyhow::{Context, Result};
use atomic_float::AtomicF32;
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
    Started,
    Stopped,
    Error(String),
}

#[derive(Debug)]
pub struct AudioEngine {
    stop: Arc<AtomicBool>,
    level: Arc<AtomicF32>,
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
        let level = Arc::new(AtomicF32::new(0.0));
        let thread_stop = Arc::clone(&stop);
        let thread_level = Arc::clone(&level);
        let handle = thread::Builder::new()
            .name("privacy-voice-pipewire".to_string())
            .spawn(move || {
                let result = run_pipewire(
                    input_node_id,
                    dsp_params,
                    event_sender.clone(),
                    thread_stop,
                    thread_level,
                    monitor_output,
                );
                if let Err(error) = result {
                    let _ = event_sender.send(AudioEvent::Error(error.to_string()));
                }
            })
            .context("failed to spawn PipeWire thread")?;

        Ok(Self {
            stop,
            level,
            handle: Some(handle),
        })
    }

    pub fn level_meter(&self) -> Arc<AtomicF32> {
        Arc::clone(&self.level)
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
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = thread::Builder::new()
                .name("privacy-voice-pipewire-join".to_string())
                .spawn(move || {
                    let _ = handle.join();
                });
        }
    }
}
