use std::sync::Mutex;

use crate::apple_music::TrackInfo;
use crate::discord_rpc::DiscordManager;

pub struct AppState {
    pub current_track: Mutex<Option<TrackInfo>>,
    pub discord: DiscordManager,
}

impl AppState {
    pub fn new(discord: DiscordManager) -> Self {
        Self {
            current_track: Mutex::new(None),
            discord,
        }
    }
}
