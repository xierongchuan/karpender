use adw::prelude::*;
use std::{rc::Rc, sync::Arc};

use crate::{dsp::SharedDspParams, profiles, ui::layout::UiControls};

use super::{
    settings::{apply_config_to_controls, apply_config_to_dsp, save_config},
    state::SharedWindowState,
};

pub(super) fn connect_profile_handlers(
    state: &SharedWindowState,
    ui: &UiControls,
    dsp_params: &Arc<SharedDspParams>,
) {
    let state_for_profile = Rc::clone(state);
    let ui_for_profile = ui.clone();
    let params_for_profile = Arc::clone(dsp_params);
    ui.profile_dropdown
        .connect_selected_notify(move |dropdown| {
            if state_for_profile.borrow().updating_controls {
                return;
            }

            let selected = dropdown.selected() as usize;
            if selected == profiles::MANUAL_PROFILE_INDEX {
                state_for_profile.borrow_mut().config.active_profile_id = None;
                save_config(&state_for_profile.borrow().config, &ui_for_profile);
                update_profile_delete_sensitivity(&state_for_profile, &ui_for_profile);
                return;
            }

            let Some(profile) =
                profiles::profile_for_index(&state_for_profile.borrow().config, selected)
            else {
                return;
            };

            {
                let mut state = state_for_profile.borrow_mut();
                state.updating_controls = true;
                state.config.apply_profile(&profile);
                save_config(&state.config, &ui_for_profile);
            }

            apply_config_to_dsp(&state_for_profile.borrow().config, &params_for_profile);
            apply_config_to_controls(&state_for_profile.borrow().config, &ui_for_profile);
            state_for_profile.borrow_mut().updating_controls = false;
            update_profile_delete_sensitivity(&state_for_profile, &ui_for_profile);
        });

    let state_for_add_profile = Rc::clone(state);
    let ui_for_add_profile = ui.clone();
    ui.add_profile_button.connect_clicked(move |_| {
        {
            let mut state = state_for_add_profile.borrow_mut();
            let number = profiles::next_custom_number(&state.config);
            let id = format!("custom-profile-{number}");
            let name = format!("Custom Profile {number}");
            let profile = state.config.profile_from_current(id.clone(), name);

            state.config.profiles.push(profile);
            state.config.active_profile_id = Some(id);
            save_config(&state.config, &ui_for_add_profile);
        }

        rebuild_profile_dropdown(&state_for_add_profile, &ui_for_add_profile);
    });

    let state_for_delete_profile = Rc::clone(state);
    let ui_for_delete_profile = ui.clone();
    ui.delete_profile_button.connect_clicked(move |_| {
        let selected = ui_for_delete_profile.profile_dropdown.selected() as usize;
        let Some(index) =
            profiles::custom_index(&state_for_delete_profile.borrow().config, selected)
        else {
            return;
        };

        {
            let mut state = state_for_delete_profile.borrow_mut();
            state.config.profiles.remove(index);
            state.config.active_profile_id = None;
            save_config(&state.config, &ui_for_delete_profile);
        }

        rebuild_profile_dropdown(&state_for_delete_profile, &ui_for_delete_profile);
    });
}

pub(super) fn rebuild_profile_dropdown(state: &SharedWindowState, ui: &UiControls) {
    let labels = profiles::labels(&state.borrow().config);
    let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
    let model = gtk::StringList::new(&label_refs);
    ui.profile_dropdown.set_model(Some(&model));

    let selected = profiles::selected_index(&state.borrow().config);
    {
        let mut state = state.borrow_mut();
        state.updating_controls = true;
    }
    ui.profile_dropdown.set_selected(selected as u32);
    state.borrow_mut().updating_controls = false;
    update_profile_delete_sensitivity(state, ui);
}

pub(super) fn select_manual_profile(state: &SharedWindowState, ui: &UiControls) {
    {
        let mut state = state.borrow_mut();
        state.updating_controls = true;
    }
    ui.profile_dropdown
        .set_selected(profiles::MANUAL_PROFILE_INDEX as u32);
    state.borrow_mut().updating_controls = false;
    update_profile_delete_sensitivity(state, ui);
}

fn update_profile_delete_sensitivity(state: &SharedWindowState, ui: &UiControls) {
    let selected = ui.profile_dropdown.selected() as usize;
    ui.delete_profile_button
        .set_sensitive(profiles::custom_index(&state.borrow().config, selected).is_some());
}
