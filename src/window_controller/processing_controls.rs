use adw::prelude::*;
use std::{rc::Rc, sync::Arc};

use crate::{config::VoiceMode, dsp::SharedDspParams, ui::layout::UiControls};

use super::{
    profile_controls::select_manual_profile, settings::save_config_debounced,
    state::SharedWindowState,
};

pub(super) fn connect_processing_handlers(
    state: &SharedWindowState,
    ui: &UiControls,
    dsp_params: Arc<SharedDspParams>,
) {
    let state_for_voice_mode = Rc::clone(state);
    let ui_for_voice_mode = ui.clone();
    let params_for_voice_mode = Arc::clone(&dsp_params);
    ui.voice_mode.connect_selected_notify(move |dropdown| {
        if state_for_voice_mode.borrow().updating_controls {
            return;
        }

        let mode = VoiceMode::from_index(dropdown.selected() as usize);
        params_for_voice_mode.set_voice_mode(mode);
        state_for_voice_mode.borrow_mut().config.voice_mode = mode;
        state_for_voice_mode.borrow_mut().config.active_profile_id = None;
        save_config_debounced(&state_for_voice_mode.borrow().config, &ui_for_voice_mode);
        select_manual_profile(&state_for_voice_mode, &ui_for_voice_mode);
    });

    let state_for_gain = Rc::clone(state);
    let ui_for_gain = ui.clone();
    let params_for_gain = Arc::clone(&dsp_params);
    ui.gain.connect_value_changed(move |scale| {
        if state_for_gain.borrow().updating_controls {
            return;
        }

        let value = scale.value() as f32;
        params_for_gain.set_gain(value);
        state_for_gain.borrow_mut().config.gain = value;
        state_for_gain.borrow_mut().config.active_profile_id = None;
        save_config_debounced(&state_for_gain.borrow().config, &ui_for_gain);
        select_manual_profile(&state_for_gain, &ui_for_gain);
    });

    let state_for_gate = Rc::clone(state);
    let ui_for_gate = ui.clone();
    let params_for_gate = Arc::clone(&dsp_params);
    ui.noise_gate.connect_value_changed(move |scale| {
        if state_for_gate.borrow().updating_controls {
            return;
        }

        let value = scale.value() as f32;
        params_for_gate.set_noise_gate(value);
        state_for_gate.borrow_mut().config.noise_gate = value;
        state_for_gate.borrow_mut().config.active_profile_id = None;
        save_config_debounced(&state_for_gate.borrow().config, &ui_for_gate);
        select_manual_profile(&state_for_gate, &ui_for_gate);
    });

    let state_for_robot = Rc::clone(state);
    let ui_for_robot = ui.clone();
    let params_for_robot = Arc::clone(&dsp_params);
    ui.robot.connect_value_changed(move |scale| {
        if state_for_robot.borrow().updating_controls {
            return;
        }

        let value = scale.value() as f32;
        params_for_robot.set_robot_amount(value);
        state_for_robot.borrow_mut().config.robot_amount = value;
        state_for_robot.borrow_mut().config.active_profile_id = None;
        save_config_debounced(&state_for_robot.borrow().config, &ui_for_robot);
        select_manual_profile(&state_for_robot, &ui_for_robot);
    });

    let state_for_monotone = Rc::clone(state);
    let ui_for_monotone = ui.clone();
    let params_for_monotone = Arc::clone(&dsp_params);
    ui.monotone.connect_active_notify(move |switch| {
        if state_for_monotone.borrow().updating_controls {
            return;
        }

        let enabled = switch.is_active();
        params_for_monotone.set_monotone(enabled);
        state_for_monotone.borrow_mut().config.monotone = enabled;
        state_for_monotone.borrow_mut().config.active_profile_id = None;
        save_config_debounced(&state_for_monotone.borrow().config, &ui_for_monotone);
        select_manual_profile(&state_for_monotone, &ui_for_monotone);
    });

    let state_for_monitor = Rc::clone(state);
    let ui_for_monitor = ui.clone();
    ui.monitor_output.connect_active_notify(move |switch| {
        state_for_monitor.borrow_mut().config.monitor_output = switch.is_active();
        save_config_debounced(&state_for_monitor.borrow().config, &ui_for_monitor);
    });
}
