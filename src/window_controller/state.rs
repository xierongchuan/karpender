use std::{cell::RefCell, rc::Rc};

use crate::{
    audio::{AudioDevice, AudioEngine},
    config::AppConfig,
};

pub(crate) type SharedWindowState = Rc<RefCell<WindowState>>;

pub(crate) struct WindowState {
    pub devices: Vec<AudioDevice>,
    pub config: AppConfig,
    pub engine: Option<AudioEngine>,
    pub updating_controls: bool,
}

impl WindowState {
    pub fn shared(config: AppConfig) -> SharedWindowState {
        Rc::new(RefCell::new(Self {
            devices: Vec::new(),
            config,
            engine: None,
            updating_controls: false,
        }))
    }
}
