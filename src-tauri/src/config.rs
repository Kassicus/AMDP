use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DisplayFormat {
    SongArtist,
    ArtistSong,
}

impl Default for DisplayFormat {
    fn default() -> Self {
        Self::SongArtist
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IdleBehavior {
    ClearStatus,
    ShowPaused,
}

impl Default for IdleBehavior {
    fn default() -> Self {
        Self::ClearStatus
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default = "default_true")]
    pub enable_on_launch: bool,
    #[serde(default = "default_true")]
    pub show_album_art: bool,
    #[serde(default = "default_true")]
    pub show_timestamps: bool,
    #[serde(default)]
    pub display_format: DisplayFormat,
    #[serde(default)]
    pub idle_behavior: IdleBehavior,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    #[serde(default)]
    pub launch_at_login: bool,
}

fn default_true() -> bool {
    true
}

fn default_poll_interval() -> u64 {
    5
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            enable_on_launch: true,
            show_album_art: true,
            show_timestamps: true,
            display_format: DisplayFormat::default(),
            idle_behavior: IdleBehavior::default(),
            poll_interval_secs: 5,
            launch_at_login: false,
        }
    }
}

pub fn config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".amdp")
        .join("config.json")
}

pub fn load_config() -> AppConfig {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config dir: {e}"))?;
    }
    let json =
        serde_json::to_string_pretty(config).map_err(|e| format!("Failed to serialize config: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to write config: {e}"))?;
    Ok(())
}
