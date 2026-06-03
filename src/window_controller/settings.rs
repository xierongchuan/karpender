use adw::prelude::*;
use gtk::glib;
use std::{
    cell::RefCell,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, TryRecvError},
    },
    thread,
    time::Duration,
};

use crate::{
    config::{self, AppConfig},
    dsp::SharedDspParams,
    ui::layout::UiControls,
};

pub(crate) fn apply_config_to_dsp(config: &AppConfig, dsp_params: &SharedDspParams) {
    dsp_params.set_gain(config.gain);
    dsp_params.set_noise_gate(config.noise_gate);
    dsp_params.set_robot_amount(config.robot_amount);
    dsp_params.set_monotone(config.monotone);
    dsp_params.set_voice_mode(config.voice_mode);
}

pub(super) fn apply_config_to_controls(config: &AppConfig, ui: &UiControls) {
    ui.voice_mode.set_selected(config.voice_mode.index() as u32);
    ui.gain.set_value(config.gain as f64);
    ui.noise_gate.set_value(config.noise_gate as f64);
    ui.robot.set_value(config.robot_amount as f64);
    ui.monotone.set_active(config.monotone);
}

pub(super) fn save_config(config: &AppConfig, ui: &UiControls) {
    cancel_pending_save();
    save_config_async(config.clone(), ui.clone());
}

pub(super) fn save_config_debounced(config: &AppConfig, ui: &UiControls) {
    cancel_pending_save();
    PENDING_SAVE.with(|pending| {
        let config = config.clone();
        let ui = ui.clone();
        let source_id = glib::timeout_add_local_once(Duration::from_millis(400), move || {
            PENDING_SAVE.with(|pending| {
                let _ = pending.borrow_mut().take();
            });
            save_config_async(config, ui);
        });
        *pending.borrow_mut() = Some(source_id);
    });
}

fn cancel_pending_save() {
    PENDING_SAVE.with(|pending| {
        if let Some(source_id) = pending.borrow_mut().take() {
            source_id.remove();
        }
    });
}

fn save_config_async(config: AppConfig, ui: UiControls) {
    let (sender, receiver) = mpsc::channel::<Result<(), String>>();
    let epoch = SAVE_EPOCH.fetch_add(1, Ordering::AcqRel) + 1;
    thread::spawn(move || {
        let result = save_latest_config(config, epoch).map_err(|error| error.to_string());
        let _ = sender.send(result);
    });

    glib::timeout_add_local(Duration::from_millis(100), move || {
        match receiver.try_recv() {
            Ok(Ok(())) => glib::ControlFlow::Break,
            Ok(Err(error)) => {
                ui.toast_overlay.add_toast(adw::Toast::new(&format!(
                    "Failed to save settings: {error}"
                )));
                glib::ControlFlow::Break
            }
            Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(TryRecvError::Disconnected) => {
                ui.toast_overlay
                    .add_toast(adw::Toast::new("Failed to save settings: worker stopped"));
                glib::ControlFlow::Break
            }
        }
    });
}

thread_local! {
    static PENDING_SAVE: RefCell<Option<glib::SourceId>> = const { RefCell::new(None) };
}

static SAVE_EPOCH: AtomicU64 = AtomicU64::new(0);
static SAVE_LOCK: Mutex<()> = Mutex::new(());

fn save_latest_config(config: AppConfig, epoch: u64) -> anyhow::Result<()> {
    let _guard = SAVE_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("settings save lock was poisoned"))?;
    if epoch != SAVE_EPOCH.load(Ordering::Acquire) {
        return Ok(());
    }

    config::save(&config)
}
