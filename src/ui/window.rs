use adw::prelude::*;
use gtk::{Orientation, glib};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, mpsc, mpsc::Receiver},
};

use crate::{
    audio::{self, AudioDevice, AudioEngine, AudioEvent},
    config::{self, AppConfig},
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
        }));

        let dsp_params = SharedDspParams::new();
        apply_config_to_dsp(&state.borrow().config, &dsp_params);

        let toast_overlay = adw::ToastOverlay::new();
        let root = gtk::Box::new(Orientation::Vertical, 0);
        toast_overlay.set_child(Some(&root));

        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&gtk::Label::new(Some("Karpender"))));
        root.append(&header);

        let content = gtk::Box::new(Orientation::Vertical, 18);
        content.set_margin_top(24);
        content.set_margin_bottom(24);
        content.set_margin_start(24);
        content.set_margin_end(24);
        root.append(&content);

        let input_row = gtk::Box::new(Orientation::Vertical, 8);
        let input_label = gtk::Label::new(Some("Input Device"));
        input_label.set_halign(gtk::Align::Start);
        let device_dropdown = gtk::DropDown::from_strings(&["No microphones found"]);
        device_dropdown.set_hexpand(true);
        input_row.append(&input_label);
        input_row.append(&device_dropdown);
        content.append(&input_row);

        let output_label = gtk::Label::new(Some("Output Virtual Mic: Karpender Privacy Voice Mic"));
        output_label.set_halign(gtk::Align::Start);
        content.append(&output_label);

        let level = gtk::LevelBar::for_interval(0.0, 1.0);
        level.set_value(0.0);
        level.set_hexpand(true);
        content.append(&labeled_widget("Level", &level));

        let gain = gtk::Scale::with_range(Orientation::Horizontal, 0.0, 4.0, 0.05);
        gain.set_value(state.borrow().config.gain as f64);
        gain.set_hexpand(true);
        content.append(&labeled_widget("Gain", &gain));

        let noise_gate = gtk::Scale::with_range(Orientation::Horizontal, 0.0, 0.4, 0.005);
        noise_gate.set_value(state.borrow().config.noise_gate as f64);
        noise_gate.set_hexpand(true);
        content.append(&labeled_widget("Noise Cleanup", &noise_gate));

        let robot = gtk::Scale::with_range(Orientation::Horizontal, 0.0, 1.0, 0.01);
        robot.set_value(state.borrow().config.robot_amount as f64);
        robot.set_hexpand(true);
        content.append(&labeled_widget("Privacy Amount", &robot));

        let monotone = gtk::Switch::builder()
            .active(state.borrow().config.monotone)
            .halign(gtk::Align::End)
            .build();
        content.append(&labeled_widget("Flatten Intonation", &monotone));

        let monitor_output = gtk::Switch::builder()
            .active(state.borrow().config.monitor_output)
            .halign(gtk::Align::End)
            .build();
        content.append(&labeled_widget("Monitor to Speakers", &monitor_output));

        let start_button = gtk::Button::with_label("Start Processing");
        start_button.add_css_class("suggested-action");
        start_button.set_hexpand(true);
        content.append(&start_button);

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Karpender")
            .default_width(460)
            .default_height(540)
            .content(&toast_overlay)
            .build();

        let ui = UiControls {
            toast_overlay,
            device_dropdown,
            level,
            gain,
            noise_gate,
            robot,
            monotone,
            monitor_output,
            start_button,
        };

        refresh_devices(&state, &ui);
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
}

#[derive(Clone)]
struct UiControls {
    toast_overlay: adw::ToastOverlay,
    device_dropdown: gtk::DropDown,
    level: gtk::LevelBar,
    gain: gtk::Scale,
    noise_gate: gtk::Scale,
    robot: gtk::Scale,
    monotone: gtk::Switch,
    monitor_output: gtk::Switch,
    start_button: gtk::Button,
}

fn labeled_widget(label: &str, widget: &impl IsA<gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::new(Orientation::Vertical, 6);
    let label = gtk::Label::new(Some(label));
    label.set_halign(gtk::Align::Start);
    row.append(&label);
    row.append(widget);
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

    let state_for_gain = Rc::clone(state);
    let params_for_gain = Arc::clone(&dsp_params);
    ui.gain.connect_value_changed(move |scale| {
        let value = scale.value() as f32;
        params_for_gain.set_gain(value);
        state_for_gain.borrow_mut().config.gain = value;
        config::save(&state_for_gain.borrow().config);
    });

    let state_for_gate = Rc::clone(state);
    let params_for_gate = Arc::clone(&dsp_params);
    ui.noise_gate.connect_value_changed(move |scale| {
        let value = scale.value() as f32;
        params_for_gate.set_noise_gate(value);
        state_for_gate.borrow_mut().config.noise_gate = value;
        config::save(&state_for_gate.borrow().config);
    });

    let state_for_robot = Rc::clone(state);
    let params_for_robot = Arc::clone(&dsp_params);
    ui.robot.connect_value_changed(move |scale| {
        let value = scale.value() as f32;
        params_for_robot.set_robot_amount(value);
        state_for_robot.borrow_mut().config.robot_amount = value;
        config::save(&state_for_robot.borrow().config);
    });

    let state_for_monotone = Rc::clone(state);
    let params_for_monotone = Arc::clone(&dsp_params);
    ui.monotone.connect_active_notify(move |switch| {
        let enabled = switch.is_active();
        params_for_monotone.set_monotone(enabled);
        state_for_monotone.borrow_mut().config.monotone = enabled;
        config::save(&state_for_monotone.borrow().config);
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
}
