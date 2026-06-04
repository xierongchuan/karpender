use adw::{Application, prelude::*};
use gtk::gio;
use std::path::Path;

use crate::ui::window::MainWindow;

const APP_ID: &str = "com.github.xierongchuan.karpender";
const APP_NAME: &str = "Karpender";
const APP_SUMMARY: &str = "PipeWire voice privacy tool with a virtual microphone";
const APP_DESCRIPTION: &str =
    "Voice privacy tool for PipeWire with anonymization DSP and a virtual microphone";
const APP_DEVELOPER: &str = "xierongchuan https://github.com/xierongchuan";
const APP_WEBSITE: &str = "https://github.com/xierongchuan/karpender";
const APP_ISSUES: &str = "https://github.com/xierongchuan/karpender/issues";
const LOCAL_ICON_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/data/icons");

pub fn run() {
    if let Err(error) = adw::init() {
        eprintln!("failed to initialize libadwaita: {error}");
        return;
    }

    register_app_icon();

    let app: Application = Application::builder().application_id(APP_ID).build();

    app.add_action_entries([gio::ActionEntry::builder("about")
        .activate(|app: &Application, _, _| {
            let about = adw::AboutDialog::new();
            about.set_application_name(APP_NAME);
            about.set_application_icon(APP_ID);
            about.set_developer_name(APP_SUMMARY);
            about.set_version(env!("CARGO_PKG_VERSION"));
            about.set_comments(APP_DESCRIPTION);
            about.set_website(APP_WEBSITE);
            about.set_issue_url(APP_ISSUES);
            about.set_license_type(gtk::License::Lgpl30);
            about.set_developers(&[APP_DEVELOPER]);
            about.present(app.active_window().as_ref());
        })
        .build()]);

    app.connect_activate(|app| {
        let window = MainWindow::new(app);
        window.present();
    });

    app.run();
}

fn register_app_icon() {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };

    gtk::IconTheme::for_display(&display).add_search_path(Path::new(LOCAL_ICON_DIR));
}
