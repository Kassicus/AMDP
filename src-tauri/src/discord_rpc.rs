use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use discord_rich_presence::activity::{Activity, ActivityType, Assets, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
use serde::Serialize;

use crate::apple_music::TrackInfo;
use crate::config::DisplayFormat;

/// Replace with your Discord Application ID.
/// Create one at https://discord.com/developers/applications
const DISCORD_APP_ID: &str = "1470809241907363921";

pub struct ActivityOptions {
    pub show_timestamps: bool,
    pub show_album_art: bool,
    pub display_format: DisplayFormat,
}

#[allow(dead_code)]
pub enum DiscordCommand {
    UpdateTrack(TrackInfo, Option<String>, ActivityOptions),
    SetPaused(TrackInfo, Option<String>, ActivityOptions),
    ClearPresence,
    Shutdown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscordStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

pub struct DiscordManager {
    tx: Sender<DiscordCommand>,
    pub status: Arc<Mutex<DiscordStatus>>,
}

impl DiscordManager {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        let status = Arc::new(Mutex::new(DiscordStatus::Disconnected));
        let thread_status = Arc::clone(&status);

        std::thread::spawn(move || {
            discord_thread_main(rx, thread_status);
        });

        Self { tx, status }
    }

    pub fn update_track(&self, track: &TrackInfo, artwork_url: Option<String>, opts: ActivityOptions) {
        let _ = self
            .tx
            .send(DiscordCommand::UpdateTrack(track.clone(), artwork_url, opts));
    }

    pub fn set_paused(&self, track: &TrackInfo, artwork_url: Option<String>, opts: ActivityOptions) {
        let _ = self
            .tx
            .send(DiscordCommand::SetPaused(track.clone(), artwork_url, opts));
    }

    pub fn clear_presence(&self) {
        let _ = self.tx.send(DiscordCommand::ClearPresence);
    }

    #[allow(dead_code)]
    pub fn shutdown(&self) {
        let _ = self.tx.send(DiscordCommand::Shutdown);
    }

    pub fn get_status(&self) -> DiscordStatus {
        self.status.lock().unwrap().clone()
    }
}

fn set_status(status: &Arc<Mutex<DiscordStatus>>, new_status: DiscordStatus) {
    *status.lock().unwrap() = new_status;
}

fn try_connect(client: &mut DiscordIpcClient) -> bool {
    client.connect().is_ok()
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Truncate a string to at most `max_len` characters (UTF-8 safe).
fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        return s;
    }
    match s.char_indices().nth(max_len) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

fn set_activity_from_track(
    client: &mut DiscordIpcClient,
    track: &TrackInfo,
    artwork_url: Option<&str>,
    opts: &ActivityOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let (details_text, state_text) = match opts.display_format {
        DisplayFormat::SongArtist => (track.name.clone(), format!("by {}", track.artist)),
        DisplayFormat::ArtistSong => (track.artist.clone(), track.name.clone()),
    };

    let large_image = if opts.show_album_art {
        artwork_url.unwrap_or("apple_music_logo")
    } else {
        "apple_music_logo"
    };

    let assets = Assets::new()
        .large_image(large_image)
        .large_text(truncate(&track.album, 128))
        .small_image("apple_music_logo")
        .small_text("Apple Music");

    let mut activity = Activity::new()
        .activity_type(ActivityType::Listening)
        .details(truncate(&details_text, 128))
        .state(truncate(&state_text, 128))
        .assets(assets);

    if opts.show_timestamps {
        let now = now_secs();
        let position_secs = track.position_secs as i64;
        let duration_secs = track.duration_secs as i64;
        let start_ts = now - position_secs;
        let end_ts = start_ts + duration_secs;
        activity = activity.timestamps(Timestamps::new().start(start_ts).end(end_ts));
    }

    client.set_activity(activity)?;
    Ok(())
}

fn set_paused_activity(
    client: &mut DiscordIpcClient,
    track: &TrackInfo,
    artwork_url: Option<&str>,
    opts: &ActivityOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let details_text = match opts.display_format {
        DisplayFormat::SongArtist => track.name.clone(),
        DisplayFormat::ArtistSong => track.artist.clone(),
    };

    let large_image = if opts.show_album_art {
        artwork_url.unwrap_or("apple_music_logo")
    } else {
        "apple_music_logo"
    };

    let assets = Assets::new()
        .large_image(large_image)
        .large_text(truncate(&track.album, 128))
        .small_image("apple_music_logo")
        .small_text("Apple Music");

    let activity = Activity::new()
        .activity_type(ActivityType::Listening)
        .details(truncate(&details_text, 128))
        .state("Paused")
        .assets(assets);

    client.set_activity(activity)?;
    Ok(())
}

fn discord_thread_main(rx: mpsc::Receiver<DiscordCommand>, status: Arc<Mutex<DiscordStatus>>) {
    let mut client = DiscordIpcClient::new(DISCORD_APP_ID);
    let mut connected = false;
    // Holds the last track so we can replay it after (re)connecting
    let mut pending_track: Option<(TrackInfo, Option<String>, bool)> = None;

    // Initial connection attempt with backoff
    set_status(&status, DiscordStatus::Connecting);
    let backoff_secs = [5, 10, 15, 30];
    for (i, &delay) in backoff_secs.iter().enumerate() {
        if try_connect(&mut client) {
            connected = true;
            set_status(&status, DiscordStatus::Connected);
            tracing::info!("Discord IPC connected");
            break;
        }
        tracing::warn!(
            "Discord connect attempt {} failed, retrying in {}s",
            i + 1,
            delay
        );
        // Check for shutdown during backoff, but stash track updates
        match rx.recv_timeout(Duration::from_secs(delay)) {
            Ok(DiscordCommand::Shutdown) => {
                set_status(&status, DiscordStatus::Disconnected);
                return;
            }
            Ok(DiscordCommand::UpdateTrack(track, art_url, _)) => {
                pending_track = Some((track, art_url, false));
            }
            Ok(DiscordCommand::SetPaused(track, art_url, _)) => {
                pending_track = Some((track, art_url, true));
            }
            Ok(DiscordCommand::ClearPresence) => {
                pending_track = None;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                set_status(&status, DiscordStatus::Disconnected);
                return;
            }
        }
    }

    if !connected {
        set_status(&status, DiscordStatus::Disconnected);
        tracing::warn!("Discord initial connection failed; will retry in background");
    }

    // Replay any track that arrived while we were connecting
    if connected {
        if let Some((ref track, ref art_url, _paused)) = pending_track {
            // On replay, use default opts since we don't store them
            let opts = ActivityOptions {
                show_timestamps: true,
                show_album_art: true,
                display_format: DisplayFormat::SongArtist,
            };
            if let Err(e) = set_activity_from_track(&mut client, track, art_url.as_deref(), &opts)
            {
                tracing::warn!("Failed to set initial Discord activity: {e}");
                connected = false;
                set_status(
                    &status,
                    DiscordStatus::Error(format!("Activity update failed: {e}")),
                );
            }
        }
    }

    // Main event loop — with exponential backoff for reconnection
    let mut reconnect_backoff = Duration::from_secs(1);

    loop {
        let timeout = if connected {
            Duration::from_secs(1)
        } else {
            reconnect_backoff
        };

        match rx.recv_timeout(timeout) {
            Ok(DiscordCommand::UpdateTrack(track, art_url, opts)) => {
                pending_track = Some((track.clone(), art_url.clone(), false));
                if !connected {
                    continue;
                }
                if let Err(e) =
                    set_activity_from_track(&mut client, &track, art_url.as_deref(), &opts)
                {
                    tracing::warn!("Failed to set Discord activity: {e}");
                    connected = false;
                    set_status(
                        &status,
                        DiscordStatus::Error(format!("Activity update failed: {e}")),
                    );
                }
            }
            Ok(DiscordCommand::SetPaused(track, art_url, opts)) => {
                pending_track = Some((track.clone(), art_url.clone(), true));
                if !connected {
                    continue;
                }
                if let Err(e) =
                    set_paused_activity(&mut client, &track, art_url.as_deref(), &opts)
                {
                    tracing::warn!("Failed to set paused Discord activity: {e}");
                    connected = false;
                    set_status(
                        &status,
                        DiscordStatus::Error(format!("Activity update failed: {e}")),
                    );
                }
            }
            Ok(DiscordCommand::ClearPresence) => {
                pending_track = None;
                if connected {
                    let _ = client.clear_activity();
                }
            }
            Ok(DiscordCommand::Shutdown) => {
                if connected {
                    let _ = client.clear_activity();
                    let _ = client.close();
                }
                set_status(&status, DiscordStatus::Disconnected);
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // If disconnected, try to reconnect with exponential backoff
                if !connected {
                    set_status(&status, DiscordStatus::Connecting);
                    if try_connect(&mut client) {
                        connected = true;
                        reconnect_backoff = Duration::from_secs(1); // reset on success
                        set_status(&status, DiscordStatus::Connected);
                        tracing::info!("Discord IPC reconnected");
                        // Replay the last known track
                        if let Some((ref track, ref art_url, paused)) = pending_track {
                            let opts = ActivityOptions {
                                show_timestamps: true,
                                show_album_art: true,
                                display_format: DisplayFormat::SongArtist,
                            };
                            let result = if paused {
                                set_paused_activity(
                                    &mut client,
                                    track,
                                    art_url.as_deref(),
                                    &opts,
                                )
                            } else {
                                set_activity_from_track(
                                    &mut client,
                                    track,
                                    art_url.as_deref(),
                                    &opts,
                                )
                            };
                            if let Err(e) = result {
                                tracing::warn!("Failed to replay Discord activity: {e}");
                                connected = false;
                                set_status(
                                    &status,
                                    DiscordStatus::Error(format!("Activity update failed: {e}")),
                                );
                            }
                        }
                    } else {
                        // Double the backoff, cap at 30s
                        reconnect_backoff = (reconnect_backoff * 2).min(Duration::from_secs(30));
                        tracing::debug!("Discord reconnect failed, next attempt in {:?}", reconnect_backoff);
                        set_status(&status, DiscordStatus::Disconnected);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // Sender dropped — clean up
                if connected {
                    let _ = client.clear_activity();
                    let _ = client.close();
                }
                set_status(&status, DiscordStatus::Disconnected);
                break;
            }
        }
    }
}
