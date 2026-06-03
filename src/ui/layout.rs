use adw::prelude::*;
use gtk::{Orientation, gio};

use crate::config::{AppConfig, VoiceMode};

pub(crate) struct WindowWidgets {
    pub window: adw::ApplicationWindow,
    pub controls: UiControls,
}

#[derive(Clone)]
pub(crate) struct UiControls {
    pub toast_overlay: adw::ToastOverlay,
    pub device_dropdown: gtk::DropDown,
    pub profile_dropdown: gtk::DropDown,
    pub add_profile_button: gtk::Button,
    pub delete_profile_button: gtk::Button,
    pub voice_mode: gtk::DropDown,
    pub level: gtk::LevelBar,
    pub gain: gtk::Scale,
    pub noise_gate: gtk::Scale,
    pub robot: gtk::Scale,
    pub monotone: gtk::Switch,
    pub monitor_output: gtk::Switch,
    pub start_button: gtk::Button,
}

pub(crate) fn build_window(
    app: &adw::Application,
    config: &AppConfig,
    load_error: Option<String>,
) -> WindowWidgets {
    let toast_overlay = adw::ToastOverlay::new();
    if let Some(error) = load_error {
        toast_overlay.add_toast(adw::Toast::new(&format!(
            "Failed to load settings: {error}"
        )));
    }

    let root = gtk::Box::new(Orientation::Vertical, 0);
    toast_overlay.set_child(Some(&root));

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&gtk::Label::new(Some("Karpender"))));
    header.pack_end(&primary_menu_button());
    root.append(&header);

    let device_dropdown = device_dropdown();
    let profile_controls = profile_controls();
    let voice_mode = voice_mode_dropdown(config.voice_mode);
    let level = level_meter();
    let gain = scale(0.0, 4.0, 0.05, config.gain);
    let noise_gate = scale(0.0, 0.4, 0.005, config.noise_gate);
    let robot = scale(0.0, 1.0, 0.01, config.robot_amount);
    let monotone = switch(config.monotone);
    let monitor_output = switch(config.monitor_output);
    let start_button = start_button();

    let page = preferences_page(PageWidgets {
        device_dropdown: &device_dropdown,
        profile_row: &profile_controls.row,
        voice_mode: &voice_mode,
        level: &level,
        gain: &gain,
        noise_gate: &noise_gate,
        robot: &robot,
        monotone: &monotone,
        monitor_output: &monitor_output,
        start_button: &start_button,
    });

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(
            &adw::Clamp::builder()
                .maximum_size(560)
                .tightening_threshold(360)
                .child(&page)
                .build(),
        )
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

    WindowWidgets {
        window,
        controls: UiControls {
            toast_overlay,
            device_dropdown,
            profile_dropdown: profile_controls.dropdown,
            add_profile_button: profile_controls.add_button,
            delete_profile_button: profile_controls.delete_button,
            voice_mode,
            level,
            gain,
            noise_gate,
            robot,
            monotone,
            monitor_output,
            start_button,
        },
    }
}

struct ProfileControls {
    row: gtk::Box,
    dropdown: gtk::DropDown,
    add_button: gtk::Button,
    delete_button: gtk::Button,
}

struct PageWidgets<'a> {
    device_dropdown: &'a gtk::DropDown,
    profile_row: &'a gtk::Box,
    voice_mode: &'a gtk::DropDown,
    level: &'a gtk::LevelBar,
    gain: &'a gtk::Scale,
    noise_gate: &'a gtk::Scale,
    robot: &'a gtk::Scale,
    monotone: &'a gtk::Switch,
    monitor_output: &'a gtk::Switch,
    start_button: &'a gtk::Button,
}

fn device_dropdown() -> gtk::DropDown {
    let dropdown = gtk::DropDown::from_strings(&["No microphones found"]);
    dropdown.set_width_request(220);
    dropdown.set_valign(gtk::Align::Center);
    dropdown
}

fn profile_controls() -> ProfileControls {
    let row = gtk::Box::new(Orientation::Horizontal, 8);
    let dropdown = gtk::DropDown::from_strings(&["Manual Settings"]);
    dropdown.set_width_request(190);
    dropdown.set_valign(gtk::Align::Center);

    let actions = gtk::Box::new(Orientation::Horizontal, 0);
    actions.add_css_class("linked");
    actions.set_valign(gtk::Align::Center);

    let add_button = gtk::Button::from_icon_name("list-add-symbolic");
    add_button.set_tooltip_text(Some("Add Profile"));
    let delete_button = gtk::Button::from_icon_name("user-trash-symbolic");
    delete_button.set_tooltip_text(Some("Delete Profile"));

    actions.append(&add_button);
    actions.append(&delete_button);
    row.append(&dropdown);
    row.append(&actions);

    ProfileControls {
        row,
        dropdown,
        add_button,
        delete_button,
    }
}

fn voice_mode_dropdown(selected: VoiceMode) -> gtk::DropDown {
    let labels = VoiceMode::ALL
        .iter()
        .map(|mode| mode.label())
        .collect::<Vec<_>>();
    let dropdown = gtk::DropDown::from_strings(&labels);
    dropdown.set_selected(selected.index() as u32);
    dropdown.set_width_request(220);
    dropdown.set_valign(gtk::Align::Center);
    dropdown
}

fn level_meter() -> gtk::LevelBar {
    let level = gtk::LevelBar::for_interval(0.0, 1.0);
    level.set_value(0.0);
    level.set_width_request(220);
    level.set_valign(gtk::Align::Center);
    level
}

fn scale(min: f64, max: f64, step: f64, value: f32) -> gtk::Scale {
    let scale = gtk::Scale::with_range(Orientation::Horizontal, min, max, step);
    scale.set_value(value as f64);
    scale.set_draw_value(false);
    scale.set_hexpand(true);
    scale.set_width_request(240);
    scale.set_valign(gtk::Align::Center);
    scale
}

fn switch(active: bool) -> gtk::Switch {
    gtk::Switch::builder()
        .active(active)
        .valign(gtk::Align::Center)
        .build()
}

fn start_button() -> gtk::Button {
    let button = gtk::Button::with_label("Start Processing");
    button.add_css_class("suggested-action");
    button.set_hexpand(true);
    button.set_margin_top(6);
    button
}

fn preferences_page(widgets: PageWidgets<'_>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();
    page.set_vexpand(true);

    let devices_group = adw::PreferencesGroup::builder()
        .title("Audio")
        .description("Select the source microphone and virtual output.")
        .build();
    devices_group.add(&action_row_with_suffix(
        "Input Device",
        widgets.device_dropdown,
    ));
    devices_group.add(
        &adw::ActionRow::builder()
            .title("Virtual Microphone")
            .subtitle("Karpender Privacy Voice Mic")
            .selectable(false)
            .build(),
    );
    devices_group.add(&action_row_with_suffix("Level", widgets.level));
    page.add(&devices_group);

    let profiles_group = adw::PreferencesGroup::builder()
        .title("Profiles")
        .description("Choose a built-in voice or save the current controls.")
        .build();
    profiles_group.add(&action_row_with_suffix("Profile", widgets.profile_row));
    profiles_group.add(&action_row_with_suffix(
        "Processing Type",
        widgets.voice_mode,
    ));
    page.add(&profiles_group);

    let controls_group = adw::PreferencesGroup::builder()
        .title("Voice Controls")
        .description("Fine tune drive, cleanup, and anonymization strength.")
        .build();
    controls_group.add(&action_row_with_suffix("Gain", widgets.gain));
    controls_group.add(&action_row_with_suffix("Noise Cleanup", widgets.noise_gate));
    controls_group.add(&action_row_with_suffix("Privacy Amount", widgets.robot));
    controls_group.add(&switch_action_row("Flatten Intonation", widgets.monotone));
    controls_group.add(&switch_action_row(
        "Monitor to Speakers",
        widgets.monitor_output,
    ));
    page.add(&controls_group);

    let session_group = adw::PreferencesGroup::new();
    session_group.add(widgets.start_button);
    page.add(&session_group);

    page
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
