use adw::prelude::*;
use gtk::{Orientation, gio};

use crate::config::{AppConfig, VoiceMode};

const CONTROL_MIN_WIDTH: i32 = 180;
const WINDOW_MIN_WIDTH: i32 = 360;
const WINDOW_MIN_HEIGHT: i32 = 480;
const WIDE_BREAKPOINT: &str = "min-width: 760px";

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

    install_styles();

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&gtk::Label::new(Some("Karpender"))));
    header.pack_end(&primary_menu_button());

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
    header.pack_start(&start_button);

    let content = dashboard_content(PageWidgets {
        device_dropdown: &device_dropdown,
        profile_row: &profile_controls.row,
        voice_mode: &voice_mode,
        level: &level,
        gain: &gain,
        noise_gate: &noise_gate,
        robot: &robot,
        monotone: &monotone,
        monitor_output: &monitor_output,
    });

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(
            &adw::Clamp::builder()
                .maximum_size(1040)
                .tightening_threshold(600)
                .child(&content)
                .build(),
        )
        .vexpand(true)
        .build();

    let root = gtk::Box::new(Orientation::Vertical, 0);
    root.append(&header);
    root.append(&scrolled);

    let breakpoint_bin = adw::BreakpointBin::builder()
        .child(&root)
        .width_request(WINDOW_MIN_WIDTH)
        .height_request(WINDOW_MIN_HEIGHT)
        .build();
    configure_responsive_layout(&breakpoint_bin, &content);
    toast_overlay.set_child(Some(&breakpoint_bin));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Karpender")
        .default_width(900)
        .default_height(640)
        .content(&toast_overlay)
        .build();
    install_start_stop_shortcut(&window, &start_button);

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
}

fn device_dropdown() -> gtk::DropDown {
    let dropdown = gtk::DropDown::from_strings(&["No microphones found"]);
    configure_dropdown(&dropdown);
    dropdown
}

fn profile_controls() -> ProfileControls {
    let row = gtk::Box::new(Orientation::Horizontal, 8);
    let dropdown = gtk::DropDown::from_strings(&["Manual Settings"]);
    configure_dropdown(&dropdown);

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
    configure_dropdown(&dropdown);
    dropdown
}

fn level_meter() -> gtk::LevelBar {
    let level = gtk::LevelBar::for_interval(0.0, 1.0);
    level.set_value(0.0);
    level.set_hexpand(true);
    level.set_width_request(CONTROL_MIN_WIDTH);
    level.set_valign(gtk::Align::Center);
    level
}

fn scale(min: f64, max: f64, step: f64, value: f32) -> gtk::Scale {
    let scale = gtk::Scale::with_range(Orientation::Horizontal, min, max, step);
    scale.set_value(value as f64);
    scale.set_draw_value(false);
    scale.set_hexpand(true);
    scale.set_width_request(CONTROL_MIN_WIDTH);
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
    let button = gtk::Button::from_icon_name("media-playback-start-symbolic");
    button.add_css_class("suggested-action");
    button.set_tooltip_text(Some("Start Processing (Ctrl+K)"));
    button
}

fn dashboard_content(widgets: PageWidgets<'_>) -> gtk::Box {
    let content = gtk::Box::new(Orientation::Vertical, 16);
    content.add_css_class("karpender-content");
    content.set_vexpand(true);

    let left_pane = gtk::Box::new(Orientation::Vertical, 16);
    left_pane.set_hexpand(true);
    left_pane.set_vexpand(false);

    let right_pane = gtk::Box::new(Orientation::Vertical, 16);
    right_pane.set_hexpand(true);
    right_pane.set_vexpand(false);

    left_pane.append(&audio_group(widgets.device_dropdown, widgets.level));
    left_pane.append(&profiles_group(widgets.profile_row, widgets.voice_mode));
    right_pane.append(&controls_group(
        widgets.gain,
        widgets.noise_gate,
        widgets.robot,
        widgets.monotone,
        widgets.monitor_output,
    ));

    content.append(&left_pane);
    content.append(&right_pane);

    content
}

fn audio_group(device_dropdown: &gtk::DropDown, level: &gtk::LevelBar) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title("Audio")
        .description("Input, live level, and the virtual microphone exposed to apps.")
        .build();
    group.add(&control_row("Input Device", device_dropdown));
    group.add(
        &adw::ActionRow::builder()
            .title("Virtual Microphone")
            .subtitle("Karpender Privacy Voice Mic")
            .selectable(false)
            .build(),
    );
    group.add(&control_row("Level", level));
    group
}

fn profiles_group(profile_row: &gtk::Box, voice_mode: &gtk::DropDown) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title("Profiles")
        .description("Choose a built-in voice or save the current controls.")
        .build();
    group.add(&control_row("Profile", profile_row));
    group.add(&control_row("Processing Type", voice_mode));
    group
}

fn controls_group(
    gain: &gtk::Scale,
    noise_gate: &gtk::Scale,
    robot: &gtk::Scale,
    monotone: &gtk::Switch,
    monitor_output: &gtk::Switch,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title("Voice Controls")
        .description("Fine tune drive, cleanup, and anonymization strength.")
        .build();
    group.add(&control_row("Gain", gain));
    group.add(&control_row("Noise Cleanup", noise_gate));
    group.add(&control_row("Privacy Amount", robot));
    group.add(&switch_action_row("Flatten Intonation", monotone));
    group.add(&switch_action_row("Monitor to Speakers", monitor_output));
    group
}

fn configure_responsive_layout(breakpoint_bin: &adw::BreakpointBin, content: &gtk::Box) {
    set_compact_layout(content);

    let condition = adw::BreakpointCondition::parse(WIDE_BREAKPOINT)
        .expect("wide layout breakpoint condition should be valid");
    let breakpoint = adw::Breakpoint::new(condition);

    let wide_content = content.clone();
    breakpoint.connect_apply(move |_| {
        set_wide_layout(&wide_content);
    });

    let compact_content = content.clone();
    breakpoint.connect_unapply(move |_| {
        set_compact_layout(&compact_content);
    });

    breakpoint_bin.add_breakpoint(breakpoint);
}

fn set_wide_layout(content: &gtk::Box) {
    content.set_orientation(Orientation::Horizontal);
    content.set_homogeneous(true);
    content.set_spacing(24);
    content.set_margin_top(24);
    content.set_margin_bottom(24);
    content.set_margin_start(24);
    content.set_margin_end(24);
}

fn set_compact_layout(content: &gtk::Box) {
    content.set_orientation(Orientation::Vertical);
    content.set_homogeneous(false);
    content.set_spacing(16);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
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

fn install_start_stop_shortcut(window: &adw::ApplicationWindow, start_button: &gtk::Button) {
    let controller = gtk::ShortcutController::new();
    controller.set_scope(gtk::ShortcutScope::Global);

    let trigger = gtk::KeyvalTrigger::new(gtk::gdk::Key::k, gtk::gdk::ModifierType::CONTROL_MASK);
    let start_button = start_button.clone();
    let action = gtk::CallbackAction::new(move |_, _| {
        if start_button.is_sensitive() {
            start_button.emit_activate();
        }
        gtk::glib::Propagation::Stop
    });

    controller.add_shortcut(gtk::Shortcut::new(Some(trigger), Some(action)));
    window.add_controller(controller);
}

fn configure_dropdown(dropdown: &gtk::DropDown) {
    let factory = ellipsizing_string_factory();
    let list_factory = ellipsizing_string_factory();
    dropdown.set_factory(Some(&factory));
    dropdown.set_list_factory(Some(&list_factory));
    dropdown.set_hexpand(true);
    dropdown.set_width_request(CONTROL_MIN_WIDTH);
    dropdown.set_valign(gtk::Align::Center);
}

fn ellipsizing_string_factory() -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();

    factory.connect_setup(|_, list_item| {
        let label = gtk::Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .single_line_mode(true)
            .xalign(0.0)
            .hexpand(true)
            .build();
        list_item.set_child(Some(&label));
    });

    factory.connect_bind(|_, list_item| {
        let Some(item) = list_item.item() else {
            return;
        };
        let Ok(string_object) = item.downcast::<gtk::StringObject>() else {
            return;
        };
        let Some(child) = list_item.child() else {
            return;
        };
        let Ok(label) = child.downcast::<gtk::Label>() else {
            return;
        };

        label.set_label(&string_object.string());
    });

    factory
}

fn control_row(title: &str, widget: &impl IsA<gtk::Widget>) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    row.set_activatable(false);

    let content = gtk::Box::new(Orientation::Vertical, 8);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    content.set_margin_start(12);
    content.set_margin_end(12);

    let label = gtk::Label::new(Some(title));
    label.add_css_class("caption-heading");
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);

    widget.set_hexpand(true);
    widget.set_halign(gtk::Align::Fill);
    widget.set_valign(gtk::Align::Center);

    content.append(&label);
    content.append(widget);
    row.set_child(Some(&content));

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

fn install_styles() {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };

    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        "
        .karpender-content {
            background: transparent;
        }

        ",
    );
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
