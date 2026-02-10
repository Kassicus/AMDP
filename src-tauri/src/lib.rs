mod album_art;
mod apple_music;
mod commands;
mod config;
mod discord_rpc;
mod state;
mod tray;

use std::sync::{Arc, Mutex};
use std::time::Instant;

use config::{AppConfig, IdleBehavior};
use discord_rpc::{ActivityOptions, DiscordManager};
use state::AppState;
use tauri::{ActivationPolicy, AppHandle, Emitter, Manager};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tokio::time::{sleep, Duration};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn init_tracing() -> tracing_appender::non_blocking::WorkerGuard {
    let log_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".amdp")
        .join("logs");

    // Ensure log directory exists
    let _ = std::fs::create_dir_all(&log_dir);

    // Clean up log files older than 7 days
    cleanup_old_logs(&log_dir, 7);

    let file_appender = tracing_appender::rolling::daily(&log_dir, "amdp.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_env("AMDP_LOG")
        .unwrap_or_else(|_| EnvFilter::new("amdp=info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_target(false))
        .with(fmt::layer().with_target(false).with_ansi(false).with_writer(non_blocking))
        .init();

    guard
}

fn cleanup_old_logs(log_dir: &std::path::Path, max_age_days: u64) {
    let Ok(entries) = std::fs::read_dir(log_dir) else {
        return;
    };

    let cutoff = std::time::SystemTime::now()
        - std::time::Duration::from_secs(max_age_days * 24 * 60 * 60);

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("log")
            || path.to_string_lossy().contains("amdp.log.")
        {
            if let Ok(metadata) = path.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if modified < cutoff {
                        let _ = std::fs::remove_file(&path);
                        tracing::info!("Removed old log file: {}", path.display());
                    }
                }
            }
        }
    }
}

fn truncate_tray_label(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_len.saturating_sub(1)).collect();
    format!("{truncated}\u{2026}")
}

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
        let mut last_poll = Instant::now();

        loop {
            let cfg = read_config_snapshot(&app_handle);
            sleep(Duration::from_secs(cfg.poll_interval_secs)).await;

            // Sleep/wake detection
            let elapsed = last_poll.elapsed();
            let expected = Duration::from_secs(cfg.poll_interval_secs);
            if elapsed > expected + Duration::from_secs(10) {
                tracing::info!(
                    "System wake detected (elapsed {:.1}s, expected {:.1}s) — forcing re-sync",
                    elapsed.as_secs_f64(),
                    expected.as_secs_f64()
                );
                previous = None;
            }
            last_poll = Instant::now();

            let result = tokio::task::spawn_blocking(apple_music::get_current_track)
                .await
                .ok()
                .and_then(|r| r.ok());

            tracing::debug!("Poll result: {:?}", result.as_ref().map(|t| &t.name));

            let changed = tracks_meaningfully_different(&previous, &result);

            // Always update state with latest info
            {
                let state = app_handle.state::<AppState>();
                let mut current = state.current_track.lock().unwrap();
                *current = result.clone();
            }

            if changed {
                if let Some(ref track) = result {
                    tracing::info!(
                        "Track changed: \"{}\" by {} ({})",
                        track.name,
                        track.artist,
                        if track.is_playing { "playing" } else { "paused" }
                    );
                } else {
                    tracing::info!("Track changed: nothing playing");
                }

                // Update tray now-playing label
                {
                    let state = app_handle.state::<AppState>();
                    let guard = state.now_playing_item.lock().unwrap();
                    if let Some(item) = guard.as_ref() {
                        let label = match &result {
                            Some(track) => {
                                let full = format!("{} \u{2014} {}", track.name, track.artist);
                                truncate_tray_label(&full, 50)
                            }
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
    let _guard = init_tracing();

    tracing::info!("AMDP starting up");

    let discord = DiscordManager::start();
    let loaded_config = config::load_config();
    let config = Arc::new(Mutex::new(loaded_config));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
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

            // Delayed update check (10 seconds after launch)
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(10)).await;
                check_for_updates(app_handle).await;
            });

            start_polling(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

async fn check_for_updates(app: AppHandle) {
    use tauri_plugin_updater::UpdaterExt;

    tracing::info!("Checking for updates...");
    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!("Failed to create updater: {e}");
            return;
        }
    };
    match updater.check().await {
        Ok(Some(update)) => {
            let version = update.version.clone();
            tracing::info!("Update available: v{version}");

            // Update tray item text
            let state = app.state::<AppState>();
            let guard = state.update_item.lock().unwrap();
            if let Some(item) = guard.as_ref() {
                let _ = item.set_text(format!("Update Available (v{version})"));
            }
            drop(guard);

            *state.update_available.lock().unwrap() = Some(version);
        }
        Ok(None) => {
            tracing::info!("No updates available");
        }
        Err(e) => {
            tracing::warn!("Update check failed: {e}");
        }
    }
}
