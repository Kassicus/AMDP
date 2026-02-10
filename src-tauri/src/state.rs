use std::sync::{Arc, Mutex};

use tauri::menu::{CheckMenuItem, MenuItem};
use tauri::Wry;

use crate::apple_music::TrackInfo;
use crate::config::AppConfig;
use crate::discord_rpc::DiscordManager;

pub struct AppState {
    pub current_track: Mutex<Option<TrackInfo>>,
    pub discord: DiscordManager,
    pub config: Arc<Mutex<AppConfig>>,
    pub now_playing_item: Mutex<Option<MenuItem<Wry>>>,
    pub toggle_presence_item: Mutex<Option<CheckMenuItem<Wry>>>,
    pub update_item: Mutex<Option<MenuItem<Wry>>>,
    pub update_available: Mutex<Option<String>>,
}

impl AppState {
    pub fn new(discord: DiscordManager, config: Arc<Mutex<AppConfig>>) -> Self {
        Self {
            current_track: Mutex::new(None),
            discord,
            config,
            now_playing_item: Mutex::new(None),
            toggle_presence_item: Mutex::new(None),
            update_item: Mutex::new(None),
            update_available: Mutex::new(None),
        }
    }
}
