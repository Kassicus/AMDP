use tauri::State;

use crate::apple_music::TrackInfo;
use crate::state::AppState;

#[tauri::command]
pub fn get_current_track(state: State<AppState>) -> Option<TrackInfo> {
    state.current_track.lock().unwrap().clone()
}
