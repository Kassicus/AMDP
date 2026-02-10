use tauri::State;

use crate::apple_music::TrackInfo;
use crate::discord_rpc::DiscordStatus;
use crate::state::AppState;

#[tauri::command]
pub fn get_current_track(state: State<AppState>) -> Option<TrackInfo> {
    state.current_track.lock().unwrap().clone()
}

#[tauri::command]
pub fn get_discord_status(state: State<AppState>) -> DiscordStatus {
    state.discord.get_status()
}
