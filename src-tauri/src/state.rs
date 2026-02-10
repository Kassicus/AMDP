use std::sync::Mutex;

use crate::apple_music::TrackInfo;

pub struct AppState {
    pub current_track: Mutex<Option<TrackInfo>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            current_track: Mutex::new(None),
        }
    }
}
