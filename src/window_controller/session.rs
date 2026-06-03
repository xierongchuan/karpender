use adw::prelude::*;
use atomic_float::AtomicF32;
use gtk::glib;
use std::{
    rc::Rc,
    sync::{
        Arc,
        atomic::Ordering,
        mpsc::{self, Receiver, TryRecvError},
    },
    thread,
    time::Duration,
};

use crate::{
    audio::{AudioEngine, AudioEvent},
    dsp::SharedDspParams,
    ui::layout::UiControls,
};

use super::state::SharedWindowState;

pub(super) fn connect_start_stop(
    state: &SharedWindowState,
    ui: &UiControls,
    dsp_params: Arc<SharedDspParams>,
) {
    let state_for_button = Rc::clone(state);
    let ui_for_button = ui.clone();
    ui.start_button.connect_clicked(move |_| {
        if state_for_button.borrow().engine.is_some() {
            stop_engine(&state_for_button, &ui_for_button);
            return;
        }

        let Some(device_id) = selected_device_id(&state_for_button, &ui_for_button) else {
            ui_for_button
                .toast_overlay
                .add_toast(adw::Toast::new("Select an input microphone first"));
            return;
        };

        set_starting_button_appearance(&ui_for_button);
        ui_for_button.start_button.set_sensitive(false);

        let (sender, receiver) = mpsc::channel::<AudioEvent>();
        let monitor_output = state_for_button.borrow().config.monitor_output;
        match AudioEngine::start(device_id, Arc::clone(&dsp_params), sender, monitor_output) {
            Ok(engine) => {
                let level = engine.level_meter();
                let session_id = {
                    let mut state = state_for_button.borrow_mut();
                    state.session_id = state.session_id.wrapping_add(1);
                    state.engine = Some(engine);
                    state.session_id
                };
                set_running_button_appearance(&ui_for_button);
                ui_for_button.start_button.set_sensitive(true);
                attach_audio_events(
                    receiver,
                    level,
                    session_id,
                    &state_for_button,
                    &ui_for_button,
                );
            }
            Err(error) => {
                set_stopped_ui(&ui_for_button);
                ui_for_button
                    .toast_overlay
                    .add_toast(adw::Toast::new(&format!("Failed to start audio: {error}")));
            }
        }
    });
}

fn attach_audio_events(
    receiver: Receiver<AudioEvent>,
    level: Arc<AtomicF32>,
    session_id: u64,
    state: &SharedWindowState,
    ui: &UiControls,
) {
    let state = Rc::clone(state);
    let ui = ui.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(33), move || {
        if state.borrow().session_id != session_id {
            return glib::ControlFlow::Break;
        }

        ui.level.set_value(level.load(Ordering::Relaxed) as f64);
        loop {
            match receiver.try_recv() {
                Ok(AudioEvent::Started) => {}
                Ok(AudioEvent::Stopped) => {
                    state.borrow_mut().engine = None;
                    set_stopped_ui(&ui);
                    return glib::ControlFlow::Break;
                }
                Ok(AudioEvent::Error(error)) => {
                    if let Some(engine) = state.borrow_mut().engine.take() {
                        stop_engine_in_background(engine);
                    }
                    set_stopped_ui(&ui);
                    ui.toast_overlay
                        .add_toast(adw::Toast::new(&format!("Audio error: {error}")));
                    return glib::ControlFlow::Break;
                }
                Err(TryRecvError::Empty) => return glib::ControlFlow::Continue,
                Err(TryRecvError::Disconnected) => {
                    state.borrow_mut().engine = None;
                    set_stopped_ui(&ui);
                    return glib::ControlFlow::Break;
                }
            };
        }
    });
}

fn stop_engine(state: &SharedWindowState, ui: &UiControls) {
    set_stopping_button_appearance(ui);
    ui.start_button.set_sensitive(false);
    ui.level.set_value(0.0);

    let (mut engine, session_id) = {
        let mut state = state.borrow_mut();
        state.session_id = state.session_id.wrapping_add(1);
        (state.engine.take(), state.session_id)
    };

    if let Some(mut engine) = engine.take() {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            engine.stop();
            let _ = sender.send(());
        });

        let ui = ui.clone();
        let state = Rc::clone(state);
        glib::timeout_add_local(Duration::from_millis(50), move || {
            match receiver.try_recv() {
                Ok(()) | Err(TryRecvError::Disconnected) => {
                    if state.borrow().session_id == session_id {
                        set_stopped_ui(&ui);
                    }
                    glib::ControlFlow::Break
                }
                Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
            }
        });
    }
}

fn set_stopped_ui(ui: &UiControls) {
    set_start_button_appearance(ui);
    ui.start_button.set_sensitive(true);
    ui.level.set_value(0.0);
}

fn set_start_button_appearance(ui: &UiControls) {
    ui.start_button
        .set_icon_name("media-playback-start-symbolic");
    ui.start_button
        .set_tooltip_text(Some("Start Processing (Ctrl+K)"));
    ui.start_button.remove_css_class("destructive-action");
    ui.start_button.add_css_class("suggested-action");
}

fn set_starting_button_appearance(ui: &UiControls) {
    ui.start_button
        .set_icon_name("media-playback-start-symbolic");
    ui.start_button
        .set_tooltip_text(Some("Starting... (Ctrl+K)"));
    ui.start_button.remove_css_class("destructive-action");
    ui.start_button.add_css_class("suggested-action");
}

fn set_running_button_appearance(ui: &UiControls) {
    ui.start_button
        .set_icon_name("media-playback-stop-symbolic");
    ui.start_button
        .set_tooltip_text(Some("Stop Processing (Ctrl+K)"));
    ui.start_button.remove_css_class("suggested-action");
    ui.start_button.add_css_class("destructive-action");
}

fn set_stopping_button_appearance(ui: &UiControls) {
    ui.start_button
        .set_icon_name("media-playback-stop-symbolic");
    ui.start_button
        .set_tooltip_text(Some("Stopping... (Ctrl+K)"));
    ui.start_button.remove_css_class("suggested-action");
    ui.start_button.add_css_class("destructive-action");
}

fn stop_engine_in_background(mut engine: AudioEngine) {
    thread::spawn(move || engine.stop());
}

fn selected_device_id(state: &SharedWindowState, ui: &UiControls) -> Option<u32> {
    state
        .borrow()
        .devices
        .get(ui.device_dropdown.selected() as usize)
        .map(|device| device.id)
}
