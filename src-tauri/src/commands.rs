use tauri::{AppHandle, Emitter, State};
use tauri_plugin_autostart::ManagerExt;

use crate::apple_music::TrackInfo;
use crate::config::{self, AppConfig};
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

#[tauri::command]
pub fn get_config(state: State<AppState>) -> AppConfig {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
pub fn save_config(
    app: AppHandle,
    state: State<AppState>,
    mut new_config: AppConfig,
) -> Result<(), String> {
    // Clamp poll interval to valid range
    new_config.poll_interval_secs = new_config.poll_interval_secs.clamp(2, 15);

    // Detect launch_at_login change
    let old_launch_at_login = {
        let cfg = state.config.lock().unwrap();
        cfg.launch_at_login
    };

    if new_config.launch_at_login != old_launch_at_login {
        let autolaunch = app.autolaunch();
        if new_config.launch_at_login {
            autolaunch.enable().map_err(|e| format!("Failed to enable autostart: {e}"))?;
        } else {
            autolaunch.disable().map_err(|e| format!("Failed to disable autostart: {e}"))?;
        }
    }

    // Detect enable_on_launch change for tray sync
    let old_enabled = {
        let cfg = state.config.lock().unwrap();
        cfg.enable_on_launch
    };

    // Write to state
    {
        let mut cfg = state.config.lock().unwrap();
        *cfg = new_config.clone();
    }

    // Persist to disk
    config::save_config(&new_config)?;

    // Sync tray checkbox if presence toggle changed
    if new_config.enable_on_launch != old_enabled {
        if let Some(item) = state.toggle_presence_item.lock().unwrap().as_ref() {
            let _ = item.set_checked(new_config.enable_on_launch);
        }
    }

    // If presence disabled, clear Discord
    if !new_config.enable_on_launch {
        state.discord.clear_presence();
    }

    let _ = app.emit("config-changed", ());
    Ok(())
}
