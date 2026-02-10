mod album_art;
mod apple_music;
mod commands;
mod discord_rpc;
mod state;

use discord_rpc::DiscordManager;
use state::AppState;
use tauri::{AppHandle, Emitter, Manager};
use tokio::time::{interval, Duration};

fn tracks_meaningfully_different(
    a: &Option<apple_music::TrackInfo>,
    b: &Option<apple_music::TrackInfo>,
) -> bool {
    match (a, b) {
        (None, None) => false,
        (Some(_), None) | (None, Some(_)) => true,
        (Some(a), Some(b)) => {
            a.name != b.name || a.artist != b.artist || a.album != b.album || a.is_playing != b.is_playing
        }
    }
}

fn start_polling(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = interval(Duration::from_secs(5));
        let mut previous: Option<apple_music::TrackInfo> = None;
        let mut art_resolver = album_art::AlbumArtResolver::new();

        loop {
            ticker.tick().await;

            let result = tokio::task::spawn_blocking(apple_music::get_current_track)
                .await
                .ok()
                .and_then(|r| r.ok());

            let changed = tracks_meaningfully_different(&previous, &result);

            // Always update state with latest info (including position)
            {
                let state = app_handle.state::<AppState>();
                let mut current = state.current_track.lock().unwrap();
                *current = result.clone();
            }

            if changed {
                // Update Discord presence
                {
                    let state = app_handle.state::<AppState>();
                    match &result {
                        Some(track) if track.is_playing => {
                            let artwork_url = art_resolver.resolve(&track.artist, &track.album).await;
                            state.discord.update_track(track, artwork_url);
                        }
                        _ => {
                            state.discord.clear_presence();
                        }
                    }
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

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new(discord))
        .invoke_handler(tauri::generate_handler![
            commands::get_current_track,
            commands::get_discord_status
        ])
        .setup(|app| {
            start_polling(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
