use adw::prelude::*;

use crate::ui::window::MainWindow;

const APP_ID: &str = "com.github.xierongchuan.karpender";

pub fn run() {
    adw::init().expect("failed to initialize libadwaita");

    let app = adw::Application::builder().application_id(APP_ID).build();

    app.connect_activate(|app| {
        let window = MainWindow::new(app);
        window.present();
    });

    app.run();
}
