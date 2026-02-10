import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface TrackInfo {
  name: string;
  artist: string;
  album: string;
  durationSecs: number;
  positionSecs: number;
  isPlaying: boolean;
}

function formatTime(secs: number): string {
  const minutes = Math.floor(secs / 60);
  const seconds = Math.floor(secs % 60);
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

function updateDisplay(track: TrackInfo | null) {
  const statusEl = document.getElementById("status")!;
  const trackInfoEl = document.getElementById("track-info")!;

  if (!track) {
    statusEl.textContent = "Nothing playing";
    statusEl.className = "status stopped";
    trackInfoEl.classList.add("hidden");
    return;
  }

  statusEl.textContent = track.isPlaying ? "Now Playing" : "Paused";
  statusEl.className = `status ${track.isPlaying ? "playing" : "paused"}`;
  trackInfoEl.classList.remove("hidden");

  document.getElementById("track-name")!.textContent = track.name;
  document.getElementById("track-artist")!.textContent = track.artist;
  document.getElementById("track-album")!.textContent = track.album;
  document.getElementById("track-duration")!.textContent = formatTime(
    track.durationSecs,
  );
  document.getElementById("track-position")!.textContent = formatTime(
    track.positionSecs,
  );
}

window.addEventListener("DOMContentLoaded", async () => {
  const track = await invoke<TrackInfo | null>("get_current_track");
  updateDisplay(track);

  await listen<TrackInfo | null>("track-changed", (event) => {
    updateDisplay(event.payload);
  });
});
