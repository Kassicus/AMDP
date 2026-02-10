mod album_art;
mod apple_music;
mod commands;
mod config;
mod discord_rpc;
mod state;
mod tray;

use std::sync::{Arc, Mutex};

use config::{AppConfig, IdleBehavior};
use discord_rpc::{ActivityOptions, DiscordManager};
use state::AppState;
use tauri::{ActivationPolicy, AppHandle, Emitter, Manager};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tokio::time::{sleep, Duration};

fn tracks_meaningfully_different(
    a: &Option<apple_music::TrackInfo>,
    b: &Option<apple_music::TrackInfo>,
) -> bool {
    match (a, b) {
        (None, None) => false,
        (Some(_), None) | (None, Some(_)) => true,
        (Some(a), Some(b)) => {
            a.name != b.name
                || a.artist != b.artist
                || a.album != b.album
                || a.is_playing != b.is_playing
        }
    }
}

fn read_config_snapshot(app_handle: &AppHandle) -> AppConfig {
    let state = app_handle.state::<AppState>();
    let cfg = state.config.lock().unwrap().clone();
    cfg
}

fn build_activity_options(cfg: &AppConfig) -> ActivityOptions {
    ActivityOptions {
        show_timestamps: cfg.show_timestamps,
        show_album_art: cfg.show_album_art,
        display_format: cfg.display_format,
    }
}

fn start_polling(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut previous: Option<apple_music::TrackInfo> = None;
        let mut art_resolver = album_art::AlbumArtResolver::new();

        loop {
            let cfg = read_config_snapshot(&app_handle);
            sleep(Duration::from_secs(cfg.poll_interval_secs)).await;

            let result = tokio::task::spawn_blocking(apple_music::get_current_track)
                .await
                .ok()
                .and_then(|r| r.ok());

            let changed = tracks_meaningfully_different(&previous, &result);

            // Always update state with latest info
            {
                let state = app_handle.state::<AppState>();
                let mut current = state.current_track.lock().unwrap();
                *current = result.clone();
            }

            if changed {
                // Update tray now-playing label
                {
                    let state = app_handle.state::<AppState>();
                    let guard = state.now_playing_item.lock().unwrap();
                    if let Some(item) = guard.as_ref() {
                        let label = match &result {
                            Some(track) => format!("{} — {}", track.name, track.artist),
                            None => "Not Playing".to_string(),
                        };
                        let _ = item.set_text(label);
                    }
                    drop(guard);
                }

                // Re-read config for Discord decisions
                let cfg = read_config_snapshot(&app_handle);
                let presence_enabled = cfg.enable_on_launch;

                if presence_enabled {
                    let state = app_handle.state::<AppState>();
                    match &result {
                        Some(track) if track.is_playing => {
                            let artwork_url = if cfg.show_album_art {
                                art_resolver.resolve(&track.artist, &track.album).await
                            } else {
                                None
                            };
                            let opts = build_activity_options(&cfg);
                            state.discord.update_track(track, artwork_url, opts);
                        }
                        Some(track) => {
                            // Paused
                            match cfg.idle_behavior {
                                IdleBehavior::ClearStatus => {
                                    state.discord.clear_presence();
                                }
                                IdleBehavior::ShowPaused => {
                                    let artwork_url = if cfg.show_album_art {
                                        art_resolver
                                            .resolve(&track.artist, &track.album)
                                            .await
                                    } else {
                                        None
                                    };
                                    let opts = build_activity_options(&cfg);
                                    state.discord.set_paused(track, artwork_url, opts);
                                }
                            }
                        }
                        None => {
                            state.discord.clear_presence();
                        }
                    }
                } else {
                    // Presence disabled — ensure cleared
                    let state = app_handle.state::<AppState>();
                    state.discord.clear_presence();
                }

                let _ = app_handle.emit("track-changed", &result);
                previous = result;
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let discord = DiscordManager::start();
    let loaded_config = config::load_config();
    let config = Arc::new(Mutex::new(loaded_config));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(AppState::new(discord, config))
        .invoke_handler(tauri::generate_handler![
            commands::get_current_track,
            commands::get_discord_status,
            commands::get_config,
            commands::save_config,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            // Hide from dock — menu-bar-only app
            app.set_activation_policy(ActivationPolicy::Accessory);

            tray::setup_tray(app)?;

            // Sync autostart state with config
            let state = app.state::<AppState>();
            let launch_at_login = state.config.lock().unwrap().launch_at_login;
            let autolaunch = app.autolaunch();
            if launch_at_login {
                let _ = autolaunch.enable();
            } else {
                let _ = autolaunch.disable();
            }

            start_polling(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
