use adw::prelude::*;

use crate::audio;

use super::state::SharedWindowState;
use crate::ui::layout::UiControls;

pub(super) fn refresh_devices(state: &SharedWindowState, ui: &UiControls) {
    match audio::devices::list_input_devices() {
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
