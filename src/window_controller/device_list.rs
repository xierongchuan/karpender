use adw::prelude::*;
use gtk::glib;
use std::{
    rc::Rc,
    sync::mpsc::{self, TryRecvError},
    thread,
    time::Duration,
};

use crate::audio::{self, AudioDevice};

use super::state::SharedWindowState;
use crate::ui::layout::UiControls;

pub(super) fn refresh_devices(state: &SharedWindowState, ui: &UiControls) {
    ui.start_button.set_sensitive(false);
    let (sender, receiver) = mpsc::channel::<Result<Vec<AudioDevice>, String>>();
    thread::spawn(move || {
        let result = audio::devices::list_input_devices().map_err(|error| error.to_string());
        let _ = sender.send(result);
    });

    let state = Rc::clone(state);
    let ui = ui.clone();
    glib::timeout_add_local(Duration::from_millis(50), move || {
        match receiver.try_recv() {
            Ok(result) => {
                apply_device_result(result, &state, &ui);
                glib::ControlFlow::Break
            }
            Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(TryRecvError::Disconnected) => {
                ui.start_button.set_sensitive(false);
                ui.toast_overlay.add_toast(adw::Toast::new(
                    "Failed to list microphones: worker stopped",
                ));
                glib::ControlFlow::Break
            }
        }
    });
}

fn apply_device_result(
    result: Result<Vec<AudioDevice>, String>,
    state: &SharedWindowState,
    ui: &UiControls,
) {
    match result {
        Ok(devices) => {
            let labels = devices
                .iter()
                .map(|device| device.description.as_str())
                .collect::<Vec<_>>();

            let model = gtk::StringList::new(&labels);
            ui.device_dropdown.set_model(Some(&model));

            let selected = devices
                .iter()
                .position(|device| Some(device.id) == state.borrow().config.input_node_id)
                .unwrap_or(0);
            ui.device_dropdown.set_selected(selected as u32);
            ui.start_button.set_sensitive(!devices.is_empty());

            state.borrow_mut().devices = devices;
        }
        Err(error) => {
            ui.start_button.set_sensitive(false);
            ui.toast_overlay.add_toast(adw::Toast::new(&format!(
                "Failed to list microphones: {error}"
            )));
        }
    }
}
