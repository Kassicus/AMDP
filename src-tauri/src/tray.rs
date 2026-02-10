use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, Emitter, Manager};

use crate::config;
use crate::state::AppState;

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
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &now_playing,
            &PredefinedMenuItem::separator(app)?,
            &toggle_presence,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    // Store menu item handles in state for later updates
    {
        *state.now_playing_item.lock().unwrap() = Some(now_playing);
        *state.toggle_presence_item.lock().unwrap() = Some(toggle_presence);
    }

    let icon = Image::from_bytes(include_bytes!("../icons/32x32.png"))?;

    TrayIconBuilder::new()
        .icon(icon)
        .icon_as_template(true)
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "toggle_presence" => {
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
                if let Some(window) = app.get_webview_window("settings") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}
