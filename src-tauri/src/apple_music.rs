use serde::{Deserialize, Serialize};
use std::fmt;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackInfo {
    pub name: String,
    pub artist: String,
    pub album: String,
    pub duration_secs: f64,
    pub position_secs: f64,
    pub is_playing: bool,
}

#[derive(Debug)]
pub enum AppleMusicError {
    AppNotRunning,
    ScriptExecutionFailed(String),
    ParseError(String),
}

impl fmt::Display for AppleMusicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppleMusicError::AppNotRunning => write!(f, "Music.app is not running"),
            AppleMusicError::ScriptExecutionFailed(e) => write!(f, "AppleScript failed: {e}"),
            AppleMusicError::ParseError(e) => write!(f, "Parse error: {e}"),
        }
    }
}

fn is_music_running() -> Result<bool, AppleMusicError> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(r#"tell application "System Events" to (name of processes) contains "Music""#)
        .output()
        .map_err(|e| AppleMusicError::ScriptExecutionFailed(e.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(stdout == "true")
}

pub fn get_current_track() -> Result<TrackInfo, AppleMusicError> {
    if !is_music_running()? {
        return Err(AppleMusicError::AppNotRunning);
    }

    let script = r#"
tell application "Music"
    set playerState to player state as string
    if playerState is "stopped" then
        return "stopped||||||"
    end if
    set trackName to name of current track
    set trackArtist to artist of current track
    set trackAlbum to album of current track
    set trackDuration to duration of current track
    set trackPosition to player position
    set isPlaying to (playerState is "playing")
    return trackName & "||" & trackArtist & "||" & trackAlbum & "||" & trackDuration & "||" & trackPosition & "||" & isPlaying
end tell
"#;

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| AppleMusicError::ScriptExecutionFailed(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(AppleMusicError::ScriptExecutionFailed(stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_track_response(&stdout)
}

fn parse_track_response(response: &str) -> Result<TrackInfo, AppleMusicError> {
    let parts: Vec<&str> = response.split("||").collect();

    if parts.len() < 6 {
        return Err(AppleMusicError::ParseError(format!(
            "Expected 6 fields, got {}: {response}",
            parts.len()
        )));
    }

    if parts[0] == "stopped" {
        return Err(AppleMusicError::AppNotRunning);
    }

    let duration_secs = parts[3]
        .parse::<f64>()
        .map_err(|e| AppleMusicError::ParseError(format!("Invalid duration: {e}")))?;

    let position_secs = parts[4]
        .parse::<f64>()
        .map_err(|e| AppleMusicError::ParseError(format!("Invalid position: {e}")))?;

    let is_playing = parts[5] == "true";

    Ok(TrackInfo {
        name: parts[0].to_string(),
        artist: parts[1].to_string(),
        album: parts[2].to_string(),
        duration_secs,
        position_secs,
        is_playing,
    })
}
