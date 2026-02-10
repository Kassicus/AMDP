import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface AppConfig {
  enableOnLaunch: boolean;
  showAlbumArt: boolean;
  showTimestamps: boolean;
  displayFormat: string;
  idleBehavior: string;
  pollIntervalSecs: number;
  launchAtLogin: boolean;
}

const els = {
  enableOnLaunch: () =>
    document.getElementById("enable-on-launch") as HTMLInputElement,
  showAlbumArt: () =>
    document.getElementById("show-album-art") as HTMLInputElement,
  showTimestamps: () =>
    document.getElementById("show-timestamps") as HTMLInputElement,
  displayFormat: () =>
    document.getElementById("display-format") as HTMLSelectElement,
  idleBehavior: () =>
    document.getElementById("idle-behavior") as HTMLSelectElement,
  pollInterval: () =>
    document.getElementById("poll-interval") as HTMLInputElement,
  pollIntervalValue: () =>
    document.getElementById("poll-interval-value") as HTMLSpanElement,
  launchAtLogin: () =>
    document.getElementById("launch-at-login") as HTMLInputElement,
};

function populateForm(config: AppConfig) {
  els.enableOnLaunch().checked = config.enableOnLaunch;
  els.showAlbumArt().checked = config.showAlbumArt;
  els.showTimestamps().checked = config.showTimestamps;
  els.displayFormat().value = config.displayFormat;
  els.idleBehavior().value = config.idleBehavior;
  els.pollInterval().value = String(config.pollIntervalSecs);
  els.pollIntervalValue().textContent = `${config.pollIntervalSecs}s`;
  els.launchAtLogin().checked = config.launchAtLogin;
}

function readForm(): AppConfig {
  return {
    enableOnLaunch: els.enableOnLaunch().checked,
    showAlbumArt: els.showAlbumArt().checked,
    showTimestamps: els.showTimestamps().checked,
    displayFormat: els.displayFormat().value,
    idleBehavior: els.idleBehavior().value,
    pollIntervalSecs: Number(els.pollInterval().value),
    launchAtLogin: els.launchAtLogin().checked,
  };
}

let saveTimer: ReturnType<typeof setTimeout> | null = null;

function scheduleSave() {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(async () => {
    const newConfig = readForm();
    await invoke("save_config", { newConfig });
  }, 300);
}

window.addEventListener("DOMContentLoaded", async () => {
  const config = await invoke<AppConfig>("get_config");
  populateForm(config);

  // Live slider label
  els.pollInterval().addEventListener("input", () => {
    els.pollIntervalValue().textContent = `${els.pollInterval().value}s`;
  });

  // Auto-save on any change
  const inputs = document.querySelectorAll("input, select");
  inputs.forEach((el) => {
    el.addEventListener("change", scheduleSave);
  });

  // Sync when config changes externally (e.g. tray toggle)
  await listen("config-changed", async () => {
    const updated = await invoke<AppConfig>("get_config");
    populateForm(updated);
  });
});
