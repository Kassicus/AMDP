use std::io::{BufRead, BufReader};

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Emitter, Manager};

use crate::config;
use crate::state::AppState;

/// Relaunch the app after an update by spawning `open -a` with a short delay,
/// then exiting the current process. `AppHandle::restart()` does not reliably
/// relaunch macOS menu-bar apps, so we use `open` instead.
fn relaunch_app(app: &AppHandle) {
    if let Ok(exe) = std::env::current_exe() {
        // Walk up from Contents/MacOS/binary to the .app bundle
        if let Some(bundle) = exe.parent().and_then(|p| p.parent()).and_then(|p| p.parent()) {
            let _ = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("sleep 1 && open '{}'", bundle.display()))
                .spawn();
        }
    }
    app.exit(0);
}

pub fn setup_tray(app: &App) -> tauri::Result<()> {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().unwrap().clone();

    let now_playing = MenuItem::with_id(app, "now_playing", "Not Playing", false, None::<&str>)?;
    let toggle_presence = CheckMenuItem::with_id(
        app,
        "toggle_presence",
        "Enable Rich Presence",
        true,
        cfg.enable_on_launch,
        None::<&str>,
    )?;
    let settings = MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)?;
    let copy_log = MenuItem::with_id(app, "copy_log", "Copy Debug Log", true, None::<&str>)?;
    let check_update =
        MenuItem::with_id(app, "check_update", "Check for Updates", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &now_playing,
            &PredefinedMenuItem::separator(app)?,
            &toggle_presence,
            &settings,
            &copy_log,
            &check_update,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    // Store menu item handles in state for later updates
    {
        *state.now_playing_item.lock().unwrap() = Some(now_playing);
        *state.toggle_presence_item.lock().unwrap() = Some(toggle_presence);
        *state.update_item.lock().unwrap() = Some(check_update);
    }

    let icon = Image::from_bytes(include_bytes!("../icons/32x32.png"))?;

    TrayIconBuilder::new()
        .icon(icon)
        .icon_as_template(true)
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "toggle_presence" => {
                tracing::info!("Tray: toggled Rich Presence");
                let state = app.state::<AppState>();
                let is_checked = state
                    .toggle_presence_item
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|item| item.is_checked().unwrap_or(false))
                    .unwrap_or(false);

                {
                    let mut cfg = state.config.lock().unwrap();
                    cfg.enable_on_launch = is_checked;
                    let _ = config::save_config(&cfg);
                }

                if !is_checked {
                    state.discord.clear_presence();
                }

                let _ = app.emit("config-changed", ());
            }
            "settings" => {
                tracing::info!("Tray: opening Settings");
                if let Some(window) = app.get_webview_window("settings") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "copy_log" => {
                tracing::info!("Tray: copying debug log to clipboard");
                copy_debug_log();
            }
            "check_update" => {
                tracing::info!("Tray: checking for updates");
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    use tauri_plugin_updater::UpdaterExt;

                    let updater = match app_handle.updater() {
                        Ok(u) => u,
                        Err(e) => {
                            tracing::warn!("Failed to create updater: {e}");
                            return;
                        }
                    };
                    match updater.check().await {
                        Ok(Some(update)) => {
                            let version = update.version.clone();
                            tracing::info!("Update found: v{version}, downloading...");

                            // Update tray item text
                            let state = app_handle.state::<AppState>();
                            {
                                let guard = state.update_item.lock().unwrap();
                                if let Some(item) = guard.as_ref() {
                                    let _ = item.set_text(format!("Updating to v{version}..."));
                                }
                            }

                            match update.download_and_install(|_, _| {}, || {}).await {
                                Ok(()) => {
                                    tracing::info!("Update installed, relaunching...");
                                    relaunch_app(&app_handle);
                                }
                                Err(e) => {
                                    tracing::warn!("Update install failed: {e}");
                                    let state = app_handle.state::<AppState>();
                                    let guard = state.update_item.lock().unwrap();
                                    if let Some(item) = guard.as_ref() {
                                        let _ = item.set_text("Check for Updates");
                                    }
                                }
                            }
                        }
                        Ok(None) => {
                            tracing::info!("No updates available");
                        }
                        Err(e) => {
                            tracing::warn!("Update check failed: {e}");
                        }
                    }
                });
            }
            "quit" => {
                tracing::info!("Tray: quitting");
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}

fn copy_debug_log() {
    let log_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".amdp")
        .join("logs");

    // Find the most recent log file
    let latest = match std::fs::read_dir(&log_dir) {
        Ok(entries) => entries
            .flatten()
            .filter(|e| {
                e.path()
                    .to_string_lossy()
                    .contains("amdp.log")
            })
            .max_by_key(|e| {
                e.metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            }),
        Err(_) => None,
    };

    let Some(entry) = latest else {
        tracing::warn!("No log files found in {}", log_dir.display());
        return;
    };

    // Read last 100 lines
    let path = entry.path();
    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!("Failed to open log file: {e}");
            return;
        }
    };

    let lines: Vec<String> = BufReader::new(file).lines().map_while(Result::ok).collect();
    let tail: Vec<&String> = lines.iter().rev().take(100).collect::<Vec<_>>();
    let text: String = tail.into_iter().rev().cloned().collect::<Vec<_>>().join("\n");

    match arboard::Clipboard::new() {
        Ok(mut clipboard) => {
            if let Err(e) = clipboard.set_text(&text) {
                tracing::warn!("Failed to copy to clipboard: {e}");
            } else {
                tracing::info!("Copied {} lines from log to clipboard", lines.len().min(100));
            }
        }
        Err(e) => {
            tracing::warn!("Failed to access clipboard: {e}");
        }
    }
}
