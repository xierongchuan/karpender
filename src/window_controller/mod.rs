use std::sync::Arc;

use crate::{dsp::SharedDspParams, ui::layout::UiControls};

use self::{
    device_list::refresh_devices,
    processing_controls::connect_processing_handlers,
    profile_controls::{connect_profile_handlers, rebuild_profile_dropdown},
    session::connect_start_stop,
    state::SharedWindowState,
};

mod device_list;
mod processing_controls;
mod profile_controls;
mod session;
mod settings;
pub(crate) mod state;

pub(crate) use settings::apply_config_to_dsp;

pub(crate) fn initialize_window(
    state: &SharedWindowState,
    ui: &UiControls,
    dsp_params: Arc<SharedDspParams>,
) {
    refresh_devices(state, ui);
    rebuild_profile_dropdown(state, ui);
    connect_profile_handlers(state, ui, &dsp_params);
    connect_processing_handlers(state, ui, Arc::clone(&dsp_params));
    connect_start_stop(state, ui, dsp_params);
}
