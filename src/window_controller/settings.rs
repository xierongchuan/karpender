use adw::prelude::*;

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
    if let Err(error) = config::save(config) {
        ui.toast_overlay.add_toast(adw::Toast::new(&format!(
            "Failed to save settings: {error}"
        )));
    }
}
