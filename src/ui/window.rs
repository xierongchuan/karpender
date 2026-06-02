use adw::prelude::*;
use gtk::{Orientation, gio, glib};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, mpsc, mpsc::Receiver},
};

use crate::{
    audio::{self, AudioDevice, AudioEngine, AudioEvent},
    config::{self, AppConfig, VoiceMode, VoiceProfile},
    dsp::SharedDspParams,
};

pub struct MainWindow {
    window: adw::ApplicationWindow,
}

impl MainWindow {
    pub fn new(app: &adw::Application) -> Self {
        let state = Rc::new(RefCell::new(WindowState {
            devices: Vec::new(),
            config: config::load(),
            engine: None,
            updating_controls: false,
        }));

        let dsp_params = SharedDspParams::new();
        apply_config_to_dsp(&state.borrow().config, &dsp_params);

        let toast_overlay = adw::ToastOverlay::new();
        let root = gtk::Box::new(Orientation::Vertical, 0);
        toast_overlay.set_child(Some(&root));

        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&gtk::Label::new(Some("Karpender"))));
        header.pack_end(&primary_menu_button());
        root.append(&header);

        let device_dropdown = gtk::DropDown::from_strings(&["No microphones found"]);
        device_dropdown.set_width_request(220);
        device_dropdown.set_valign(gtk::Align::Center);

        let profile_row = gtk::Box::new(Orientation::Horizontal, 8);
        let profile_dropdown = gtk::DropDown::from_strings(&["Manual Settings"]);
        profile_dropdown.set_width_request(190);
        profile_dropdown.set_valign(gtk::Align::Center);
        let profile_actions = gtk::Box::new(Orientation::Horizontal, 0);
        profile_actions.add_css_class("linked");
        profile_actions.set_valign(gtk::Align::Center);
        let add_profile_button = gtk::Button::from_icon_name("list-add-symbolic");
        add_profile_button.set_tooltip_text(Some("Add Profile"));
        let delete_profile_button = gtk::Button::from_icon_name("user-trash-symbolic");
        delete_profile_button.set_tooltip_text(Some("Delete Profile"));
        profile_row.append(&profile_dropdown);
        profile_actions.append(&add_profile_button);
        profile_actions.append(&delete_profile_button);
        profile_row.append(&profile_actions);

        let voice_mode = gtk::DropDown::from_strings(&voice_mode_labels());
        voice_mode.set_selected(voice_mode_index(state.borrow().config.voice_mode) as u32);
        voice_mode.set_width_request(220);
        voice_mode.set_valign(gtk::Align::Center);

        let level = gtk::LevelBar::for_interval(0.0, 1.0);
        level.set_value(0.0);
        level.set_width_request(220);
        level.set_valign(gtk::Align::Center);

        let gain = gtk::Scale::with_range(Orientation::Horizontal, 0.0, 4.0, 0.05);
        gain.set_value(state.borrow().config.gain as f64);
        prepare_scale(&gain);

        let noise_gate = gtk::Scale::with_range(Orientation::Horizontal, 0.0, 0.4, 0.005);
        noise_gate.set_value(state.borrow().config.noise_gate as f64);
        prepare_scale(&noise_gate);

        let robot = gtk::Scale::with_range(Orientation::Horizontal, 0.0, 1.0, 0.01);
        robot.set_value(state.borrow().config.robot_amount as f64);
        prepare_scale(&robot);

        let monotone = gtk::Switch::builder()
            .active(state.borrow().config.monotone)
            .valign(gtk::Align::Center)
            .build();

        let monitor_output = gtk::Switch::builder()
            .active(state.borrow().config.monitor_output)
            .valign(gtk::Align::Center)
            .build();

        let start_button = gtk::Button::with_label("Start Processing");
        start_button.add_css_class("suggested-action");
        start_button.set_hexpand(true);
        start_button.set_margin_top(6);

        let page = adw::PreferencesPage::new();
        page.set_vexpand(true);

        let devices_group = adw::PreferencesGroup::builder()
            .title("Audio")
            .description("Select the source microphone and virtual output.")
            .build();
        devices_group.add(&action_row_with_suffix("Input Device", &device_dropdown));
        let output_row = adw::ActionRow::builder()
            .title("Virtual Microphone")
            .subtitle("Karpender Privacy Voice Mic")
            .selectable(false)
            .build();
        devices_group.add(&output_row);
        devices_group.add(&action_row_with_suffix("Level", &level));
        page.add(&devices_group);

        let profiles_group = adw::PreferencesGroup::builder()
            .title("Profiles")
            .description("Choose a built-in voice or save the current controls.")
            .build();
        profiles_group.add(&action_row_with_suffix("Profile", &profile_row));
        profiles_group.add(&action_row_with_suffix("Processing Type", &voice_mode));
        page.add(&profiles_group);

        let controls_group = adw::PreferencesGroup::builder()
            .title("Voice Controls")
            .description("Fine tune drive, cleanup, and anonymization strength.")
            .build();
        controls_group.add(&action_row_with_suffix("Gain", &gain));
        controls_group.add(&action_row_with_suffix("Noise Cleanup", &noise_gate));
        controls_group.add(&action_row_with_suffix("Privacy Amount", &robot));
        controls_group.add(&switch_action_row("Flatten Intonation", &monotone));
        controls_group.add(&switch_action_row("Monitor to Speakers", &monitor_output));
        page.add(&controls_group);

        let session_group = adw::PreferencesGroup::new();
        session_group.add(&start_button);
        page.add(&session_group);

        let clamp = adw::Clamp::builder()
            .maximum_size(560)
            .tightening_threshold(360)
            .child(&page)
            .build();
        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&clamp)
            .vexpand(true)
            .build();
        root.append(&scrolled);

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Karpender")
            .default_width(520)
            .default_height(680)
            .content(&toast_overlay)
            .build();

        let ui = UiControls {
            toast_overlay,
            device_dropdown,
            profile_dropdown,
            add_profile_button,
            delete_profile_button,
            voice_mode,
            level,
            gain,
            noise_gate,
            robot,
            monotone,
            monitor_output,
            start_button,
        };

        refresh_devices(&state, &ui);
        rebuild_profile_dropdown(&state, &ui);
        connect_control_handlers(&state, &ui, Arc::clone(&dsp_params));
        connect_start_stop(&state, &ui, dsp_params);

        Self { window }
    }

    pub fn present(&self) {
        self.window.present();
    }
}

struct WindowState {
    devices: Vec<AudioDevice>,
    config: AppConfig,
    engine: Option<AudioEngine>,
    updating_controls: bool,
}

#[derive(Clone)]
struct UiControls {
    toast_overlay: adw::ToastOverlay,
    device_dropdown: gtk::DropDown,
    profile_dropdown: gtk::DropDown,
    add_profile_button: gtk::Button,
    delete_profile_button: gtk::Button,
    voice_mode: gtk::DropDown,
    level: gtk::LevelBar,
    gain: gtk::Scale,
    noise_gate: gtk::Scale,
    robot: gtk::Scale,
    monotone: gtk::Switch,
    monitor_output: gtk::Switch,
    start_button: gtk::Button,
}

fn prepare_scale(scale: &gtk::Scale) {
    scale.set_draw_value(false);
    scale.set_hexpand(true);
    scale.set_width_request(240);
    scale.set_valign(gtk::Align::Center);
}

fn primary_menu_button() -> gtk::MenuButton {
    let menu = gio::Menu::new();
    menu.append(Some("About Karpender"), Some("app.about"));

    gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .tooltip_text("Main Menu")
        .build()
}

fn action_row_with_suffix(title: &str, widget: &impl IsA<gtk::Widget>) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .selectable(false)
        .activatable(false)
        .build();
    widget.set_valign(gtk::Align::Center);
    row.add_suffix(widget);
    row
}

fn switch_action_row(title: &str, switch: &gtk::Switch) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .selectable(false)
        .activatable_widget(switch)
        .build();
    switch.set_valign(gtk::Align::Center);
    row.add_suffix(switch);
    row
}

fn refresh_devices(state: &Rc<RefCell<WindowState>>, ui: &UiControls) {
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

fn connect_control_handlers(
    state: &Rc<RefCell<WindowState>>,
    ui: &UiControls,
    dsp_params: Arc<SharedDspParams>,
) {
    let state_for_device = Rc::clone(state);
    ui.device_dropdown.connect_selected_notify(move |dropdown| {
        let selected = dropdown.selected() as usize;
        let mut state = state_for_device.borrow_mut();
        state.config.input_node_id = state.devices.get(selected).map(|device| device.id);
        config::save(&state.config);
    });

    let state_for_profile = Rc::clone(state);
    let ui_for_profile = ui.clone();
    let params_for_profile = Arc::clone(&dsp_params);
    ui.profile_dropdown
        .connect_selected_notify(move |dropdown| {
            if state_for_profile.borrow().updating_controls {
                return;
            }

            let selected = dropdown.selected() as usize;
            if selected == 0 {
                state_for_profile.borrow_mut().config.active_profile_id = None;
                config::save(&state_for_profile.borrow().config);
                update_profile_delete_sensitivity(&state_for_profile, &ui_for_profile);
                return;
            }

            let Some(profile) = profile_for_index(&state_for_profile.borrow().config, selected)
            else {
                return;
            };

            {
                let mut state = state_for_profile.borrow_mut();
                state.updating_controls = true;
                state.config.apply_profile(&profile);
                config::save(&state.config);
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
            let number = next_custom_profile_number(&state.config);
            let id = format!("custom-profile-{number}");
            let name = format!("Custom Profile {number}");
            let profile = state.config.profile_from_current(id.clone(), name);

            state.config.profiles.push(profile);
            state.config.active_profile_id = Some(id);
            config::save(&state.config);
        }

        rebuild_profile_dropdown(&state_for_add_profile, &ui_for_add_profile);
    });

    let state_for_delete_profile = Rc::clone(state);
    let ui_for_delete_profile = ui.clone();
    ui.delete_profile_button.connect_clicked(move |_| {
        let selected = ui_for_delete_profile.profile_dropdown.selected() as usize;
        let Some(index) = custom_profile_index(&state_for_delete_profile.borrow().config, selected)
        else {
            return;
        };

        {
            let mut state = state_for_delete_profile.borrow_mut();
            state.config.profiles.remove(index);
            state.config.active_profile_id = None;
            config::save(&state.config);
        }

        rebuild_profile_dropdown(&state_for_delete_profile, &ui_for_delete_profile);
    });

    let state_for_voice_mode = Rc::clone(state);
    let ui_for_voice_mode = ui.clone();
    let params_for_voice_mode = Arc::clone(&dsp_params);
    ui.voice_mode.connect_selected_notify(move |dropdown| {
        if state_for_voice_mode.borrow().updating_controls {
            return;
        }

        let mode = voice_mode_from_index(dropdown.selected() as usize);
        params_for_voice_mode.set_voice_mode(mode);
        state_for_voice_mode.borrow_mut().config.voice_mode = mode;
        state_for_voice_mode.borrow_mut().config.active_profile_id = None;
        config::save(&state_for_voice_mode.borrow().config);
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
        config::save(&state_for_gain.borrow().config);
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
        config::save(&state_for_gate.borrow().config);
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
        config::save(&state_for_robot.borrow().config);
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
        config::save(&state_for_monotone.borrow().config);
        select_manual_profile(&state_for_monotone, &ui_for_monotone);
    });

    let state_for_monitor = Rc::clone(state);
    ui.monitor_output.connect_active_notify(move |switch| {
        state_for_monitor.borrow_mut().config.monitor_output = switch.is_active();
        config::save(&state_for_monitor.borrow().config);
    });
}

fn connect_start_stop(
    state: &Rc<RefCell<WindowState>>,
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

fn attach_audio_events(
    receiver: Receiver<AudioEvent>,
    state: &Rc<RefCell<WindowState>>,
    ui: &UiControls,
) {
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

fn stop_engine(state: &Rc<RefCell<WindowState>>, ui: &UiControls) {
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

fn selected_device_id(state: &Rc<RefCell<WindowState>>, ui: &UiControls) -> Option<u32> {
    state
        .borrow()
        .devices
        .get(ui.device_dropdown.selected() as usize)
        .map(|device| device.id)
}

fn apply_config_to_dsp(config: &AppConfig, dsp_params: &SharedDspParams) {
    dsp_params.set_gain(config.gain);
    dsp_params.set_noise_gate(config.noise_gate);
    dsp_params.set_robot_amount(config.robot_amount);
    dsp_params.set_monotone(config.monotone);
    dsp_params.set_voice_mode(config.voice_mode);
}

fn apply_config_to_controls(config: &AppConfig, ui: &UiControls) {
    ui.voice_mode
        .set_selected(voice_mode_index(config.voice_mode) as u32);
    ui.gain.set_value(config.gain as f64);
    ui.noise_gate.set_value(config.noise_gate as f64);
    ui.robot.set_value(config.robot_amount as f64);
    ui.monotone.set_active(config.monotone);
}

fn rebuild_profile_dropdown(state: &Rc<RefCell<WindowState>>, ui: &UiControls) {
    let labels = profile_labels(&state.borrow().config);
    let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
    let model = gtk::StringList::new(&label_refs);
    ui.profile_dropdown.set_model(Some(&model));

    let selected = selected_profile_index(&state.borrow().config);
    {
        let mut state = state.borrow_mut();
        state.updating_controls = true;
    }
    ui.profile_dropdown.set_selected(selected as u32);
    state.borrow_mut().updating_controls = false;
    update_profile_delete_sensitivity(state, ui);
}

fn profile_labels(config: &AppConfig) -> Vec<String> {
    let mut labels = vec!["Manual Settings".to_string()];
    labels.extend(
        config::built_in_profiles()
            .into_iter()
            .map(|profile| profile.name),
    );
    labels.extend(config.profiles.iter().map(|profile| profile.name.clone()));
    labels
}

fn selected_profile_index(config: &AppConfig) -> usize {
    let Some(active_id) = config.active_profile_id.as_deref() else {
        return 0;
    };

    let built_ins = config::built_in_profiles();
    if let Some(index) = built_ins
        .iter()
        .position(|profile| profile.id.as_str() == active_id)
    {
        return index + 1;
    }

    config
        .profiles
        .iter()
        .position(|profile| profile.id.as_str() == active_id)
        .map(|index| index + 1 + built_ins.len())
        .unwrap_or(0)
}

fn profile_for_index(config: &AppConfig, selected: usize) -> Option<VoiceProfile> {
    let built_ins = config::built_in_profiles();
    if selected == 0 {
        return None;
    }

    let built_in_index = selected - 1;
    if built_in_index < built_ins.len() {
        return Some(built_ins[built_in_index].clone());
    }

    let custom_index = built_in_index - built_ins.len();
    config.profiles.get(custom_index).cloned()
}

fn custom_profile_index(config: &AppConfig, selected: usize) -> Option<usize> {
    let built_in_count = config::built_in_profiles().len();
    selected.checked_sub(built_in_count + 1).and_then(|index| {
        if index < config.profiles.len() {
            Some(index)
        } else {
            None
        }
    })
}

fn next_custom_profile_number(config: &AppConfig) -> usize {
    (1..)
        .find(|number| {
            let id = format!("custom-profile-{number}");
            !config.profiles.iter().any(|profile| profile.id == id)
        })
        .unwrap_or(1)
}

fn select_manual_profile(state: &Rc<RefCell<WindowState>>, ui: &UiControls) {
    {
        let mut state = state.borrow_mut();
        state.updating_controls = true;
    }
    ui.profile_dropdown.set_selected(0);
    state.borrow_mut().updating_controls = false;
    update_profile_delete_sensitivity(state, ui);
}

fn update_profile_delete_sensitivity(state: &Rc<RefCell<WindowState>>, ui: &UiControls) {
    let selected = ui.profile_dropdown.selected() as usize;
    ui.delete_profile_button
        .set_sensitive(custom_profile_index(&state.borrow().config, selected).is_some());
}

fn voice_mode_labels() -> [&'static str; 4] {
    [
        "Masked Voice",
        "Bright Stranger",
        "Deep Morph",
        "Cinematic High",
    ]
}

fn voice_mode_index(mode: VoiceMode) -> usize {
    match mode {
        VoiceMode::Masked => 0,
        VoiceMode::BrightStranger => 1,
        VoiceMode::DeepMorph => 2,
        VoiceMode::CinematicHigh => 3,
    }
}

fn voice_mode_from_index(index: usize) -> VoiceMode {
    match index {
        1 => VoiceMode::BrightStranger,
        2 => VoiceMode::DeepMorph,
        3 => VoiceMode::CinematicHigh,
        _ => VoiceMode::Masked,
    }
}
