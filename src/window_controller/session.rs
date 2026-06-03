use adw::prelude::*;
use gtk::glib;
use std::{
    rc::Rc,
    sync::{Arc, mpsc, mpsc::Receiver},
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

        ui_for_button.start_button.set_sensitive(false);
        ui_for_button.start_button.set_label("Starting...");

        let (sender, receiver) = mpsc::channel::<AudioEvent>();
        let monitor_output = state_for_button.borrow().config.monitor_output;
        match AudioEngine::start(device_id, Arc::clone(&dsp_params), sender, monitor_output) {
            Ok(engine) => {
                state_for_button.borrow_mut().engine = Some(engine);
                ui_for_button.start_button.set_sensitive(true);
                ui_for_button.start_button.set_label("Stop Processing");
                ui_for_button
                    .start_button
                    .remove_css_class("suggested-action");
                ui_for_button
                    .start_button
                    .add_css_class("destructive-action");
                attach_audio_events(receiver, &state_for_button, &ui_for_button);
            }
            Err(error) => {
                ui_for_button.start_button.set_sensitive(true);
                ui_for_button.start_button.set_label("Start Processing");
                ui_for_button
                    .toast_overlay
                    .add_toast(adw::Toast::new(&format!("Failed to start audio: {error}")));
            }
        }
    });
}

fn attach_audio_events(receiver: Receiver<AudioEvent>, state: &SharedWindowState, ui: &UiControls) {
    let state = Rc::clone(state);
    let ui = ui.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(33), move || {
        while let Ok(event) = receiver.try_recv() {
            match event {
                AudioEvent::Level(value) => ui.level.set_value(value as f64),
                AudioEvent::Started => {}
                AudioEvent::Stopped => {
                    state.borrow_mut().engine = None;
                    set_stopped_ui(&ui);
                    return glib::ControlFlow::Break;
                }
                AudioEvent::Error(error) => {
                    state.borrow_mut().engine = None;
                    set_stopped_ui(&ui);
                    ui.toast_overlay
                        .add_toast(adw::Toast::new(&format!("Audio error: {error}")));
                    return glib::ControlFlow::Break;
                }
            }
        }
        glib::ControlFlow::Continue
    });
}

fn stop_engine(state: &SharedWindowState, ui: &UiControls) {
    ui.start_button.set_sensitive(false);
    ui.start_button.set_label("Stopping...");

    if let Some(mut engine) = state.borrow_mut().engine.take() {
        engine.stop();
    }

    set_stopped_ui(ui);
}

fn set_stopped_ui(ui: &UiControls) {
    ui.start_button.set_sensitive(true);
    ui.start_button.set_label("Start Processing");
    ui.start_button.remove_css_class("destructive-action");
    ui.start_button.add_css_class("suggested-action");
    ui.level.set_value(0.0);
}

fn selected_device_id(state: &SharedWindowState, ui: &UiControls) -> Option<u32> {
    state
        .borrow()
        .devices
        .get(ui.device_dropdown.selected() as usize)
        .map(|device| device.id)
}
