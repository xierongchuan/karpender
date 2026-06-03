use adw::prelude::*;

use crate::{
    config::{self, AppConfig},
    dsp::SharedDspParams,
};

use super::layout::build_window;
use crate::window_controller::{apply_config_to_dsp, initialize_window, state::WindowState};

pub struct MainWindow {
    window: adw::ApplicationWindow,
}

impl MainWindow {
    pub fn new(app: &adw::Application) -> Self {
        let (config, load_error) = load_config_or_default();
        let state = WindowState::shared(config);

        let dsp_params = SharedDspParams::new();
        apply_config_to_dsp(&state.borrow().config, &dsp_params);

        let widgets = build_window(app, &state.borrow().config, load_error);
        initialize_window(&state, &widgets.controls, dsp_params);

        Self {
            window: widgets.window,
        }
    }

    pub fn present(&self) {
        self.window.present();
    }
}

fn load_config_or_default() -> (AppConfig, Option<String>) {
    match config::load() {
        Ok(config) => (config, None),
        Err(error) => (AppConfig::default(), Some(error.to_string())),
    }
}
