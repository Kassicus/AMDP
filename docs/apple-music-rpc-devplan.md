# 🎵 Apple Music Discord Rich Presence — Dev Plan

## Overview

A lightweight macOS menu bar / system tray application that reads the currently playing track from Apple Music and displays it as a Discord Rich Presence activity status — complete with album artwork.

**Stack:** Tauri v2 (Rust backend + web frontend) · TypeScript · AppleScript · Discord RPC · Apple Music API (via MusicKit / iTunes Search API)

---

## Why Tauri Over Electron

| Factor | Tauri | Electron |
|---|---|---|
| Binary size | ~5–10 MB | ~150+ MB |
| RAM usage | ~20–40 MB | ~100–200 MB |
| macOS WebView | Native (WKWebView) | Bundles Chromium |
| Backend language | Rust (great for IPC, polling, low-level) | Node.js |
| System tray support | Built-in (v2) | Requires extra config |
| Code signing / notarization | Supported | Supported |

For a background utility that polls every few seconds, Tauri's resource efficiency is a clear win.

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                   Tauri Application                  │
│                                                     │
│  ┌──────────────┐          ┌─────────────────────┐  │
│  │  Frontend UI  │◄────────►  Rust Backend Core   │  │
│  │  (TypeScript) │  Tauri   │                     │  │
│  │              │  IPC     │  ┌─────────────────┐ │  │
│  │  • Settings  │          │  │ AppleScript      │ │  │
│  │  • Now Playing│         │  │ Bridge           │ │  │
│  │  • Status    │          │  │ (osascript)      │ │  │
│  └──────────────┘          │  └────────┬────────┘ │  │
│                            │           │          │  │
│                            │  ┌────────▼────────┐ │  │
│                            │  │ Track Poller     │ │  │
│                            │  │ (5s interval)    │ │  │
│                            │  └────────┬────────┘ │  │
│                            │           │          │  │
│                            │  ┌────────▼────────┐ │  │
│                            │  │ Album Art        │ │  │
│                            │  │ Resolver         │ │  │
│                            │  │ (iTunes Search   │ │  │
│                            │  │  API + cache)    │ │  │
│                            │  └────────┬────────┘ │  │
│                            │           │          │  │
│                            │  ┌────────▼────────┐ │  │
│                            │  │ Discord RPC      │ │  │
│                            │  │ Client           │ │  │
│                            │  │ (IPC socket)     │ │  │
│                            │  └─────────────────┘ │  │
│                            └─────────────────────┘  │
└─────────────────────────────────────────────────────┘
```

---

## Phase 1 — Project Scaffolding & Core Polling

**Goal:** Get a Tauri v2 app that can read the current Apple Music track.

### 1.1 Initialize the project

```bash
cargo install create-tauri-app
cargo create-tauri-app apple-music-rpc --template vanilla-ts
```

- Use the vanilla TypeScript template (minimal overhead; the UI will be very simple)
- Configure for macOS target only (initially)
- Set up the system tray / menu bar icon in `tauri.conf.json`

### 1.2 AppleScript bridge (Rust side)

Create a Rust module that shells out to `osascript` to query Apple Music state.

**AppleScript to extract track info:**

```applescript
tell application "Music"
  if player state is playing then
    set trackName to name of current track
    set trackArtist to artist of current track
    set trackAlbum to album of current track
    set trackDuration to duration of current track
    set playerPos to player position
    return trackName & "||" & trackArtist & "||" & trackAlbum & "||" & trackDuration & "||" & playerPos
  else
    return "NOT_PLAYING"
  end if
end tell
```

**Rust wrapper:**

- Use `std::process::Command` to call `osascript -e "<script>"`
- Parse the `||`-delimited response into a `TrackInfo` struct
- Handle edge cases: app not running, no track loaded, paused state
- Expose via Tauri command: `#[tauri::command] fn get_current_track() -> Option<TrackInfo>`

### 1.3 Polling loop

- Spawn an async background task (via `tokio`) that polls every **5 seconds**
- Compare each poll result to the previous state — only fire updates on change
- Emit state changes to the frontend via Tauri events

### 1.4 Data model

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackInfo {
    pub name: String,
    pub artist: String,
    pub album: String,
    pub duration_secs: f64,
    pub position_secs: f64,
    pub is_playing: bool,
}
```

### Deliverables

- [ ] Tauri v2 project compiles and opens a window on macOS
- [ ] `get_current_track` command returns live data from Apple Music
- [ ] Background poller detects track changes
- [ ] Frontend displays current track info as proof of concept

---

## Phase 2 — Discord Rich Presence Integration

**Goal:** Push track info to Discord as a Rich Presence status.

### 2.1 Discord Application setup

1. Go to [Discord Developer Portal](https://discord.com/developers/applications)
2. Create a new application (e.g., "Apple Music")
3. Note the **Application ID** (this becomes the RPC client ID)
4. Under **Rich Presence → Art Assets**, upload a default Apple Music logo as a fallback image
   - Asset key: `apple_music_logo`
   - This is used when album art can't be resolved

### 2.2 Discord RPC client (Rust)

Use the [`discord-rich-presence`](https://crates.io/crates/discord-rich-presence) crate (or `discord-rpc-client`).

**Key implementation details:**

- Connect to Discord's local IPC socket (`/tmp/discord-ipc-0` on macOS)
- Handle reconnection gracefully (Discord may restart, socket may drop)
- Map `TrackInfo` to a Rich Presence activity payload:

```rust
Activity {
    details: "Song Name",           // Line 1
    state: "by Artist Name",        // Line 2
    timestamps: Timestamps {
        start: now - position_secs, // Shows elapsed time
        end: now + remaining_secs,  // Shows remaining time
    },
    assets: Assets {
        large_image: "<album_art_url_or_asset_key>",
        large_text: "Album Name",
        small_image: "apple_music_logo",
        small_text: "Apple Music",
    },
}
```

### 2.3 State machine for presence updates

```
                    ┌──────────┐
          ┌────────►│  Idle    │◄────────┐
          │         └────┬─────┘         │
     pause/stop         │ play      app quit
          │         ┌────▼─────┐         │
          ├─────────│ Playing  ├─────────┤
          │         └────┬─────┘         │
          │              │ track change  │
          │         ┌────▼─────┐         │
          └─────────│ Updating │─────────┘
                    └──────────┘
```

- **Idle:** Clear Rich Presence (no status shown)
- **Playing:** Active presence with timestamps
- **Updating:** Brief transition when track changes; update presence payload
- Only send RPC updates on state transitions (not every poll tick)

### 2.4 Timestamp handling

Discord shows a live "elapsed" or "remaining" timer. Calculate:

```
start_timestamp = current_unix_time - player_position
end_timestamp = start_timestamp + track_duration
```

This gives Discord enough info to render `0:42 / 3:15` style progress.

### Deliverables

- [ ] Discord Developer Application created with fallback art asset
- [ ] RPC client connects to Discord and sets activity
- [ ] Track changes are reflected in Discord within ~5 seconds
- [ ] Pausing/stopping clears the presence
- [ ] Timestamps show accurate elapsed/remaining time

---

## Phase 3 — Album Art Fetching & Hosting

**Goal:** Display the actual album cover in the Discord Rich Presence.

This is the trickiest part. Discord Rich Presence only supports images in two ways:

1. **Pre-uploaded Art Assets** in the Developer Portal (static, limited to ~300)
2. **External URLs** — Discord *does* support external image URLs in the `large_image` field as of recent API updates (for bot/app activities)

We'll use **approach 2** with the iTunes Search API, which returns direct image URLs that Discord can consume.

### 3.1 iTunes Search API for album art

The iTunes Search API is **free, no auth required**, and returns album artwork URLs.

**Endpoint:**

```
https://itunes.apple.com/search?term={artist}+{album}&entity=album&limit=1
```

**Response (simplified):**

```json
{
  "results": [
    {
      "artworkUrl100": "https://is1-ssl.mzstatic.com/.../100x100bb.jpg"
    }
  ]
}
```

**Important:** You can modify the URL to get higher resolution:

- Replace `100x100bb` with `512x512bb` or `1024x1024bb`
- Use `512x512bb` — good balance of quality and load time for Discord

### 3.2 Album art resolver module

```rust
pub struct AlbumArtResolver {
    cache: HashMap<String, String>,  // "artist::album" -> artwork_url
    http_client: reqwest::Client,
}

impl AlbumArtResolver {
    pub async fn resolve(&mut self, artist: &str, album: &str) -> Option<String> {
        let cache_key = format!("{}::{}", artist.to_lowercase(), album.to_lowercase());

        if let Some(url) = self.cache.get(&cache_key) {
            return Some(url.clone());
        }

        // Query iTunes Search API
        let url = self.fetch_from_itunes(artist, album).await?;

        // Cache the result
        self.cache.insert(cache_key, url.clone());
        Some(url)
    }
}
```

### 3.3 Caching strategy

- **In-memory LRU cache** (bounded to ~500 entries) for the current session
- **On-disk JSON cache** (`~/.apple-music-rpc/art-cache.json`) to persist across restarts
- Cache key: normalized `"artist::album"` string
- Cache value: the resolved artwork URL
- TTL: 30 days (album art URLs from Apple are long-lived but not permanent)

### 3.4 Fallback chain

```
1. Check in-memory cache
2. Check on-disk cache
3. Query iTunes Search API
4. If API returns no results → use the default "apple_music_logo" asset
```

### 3.5 Rate limiting

The iTunes Search API has an unofficial rate limit of ~20 requests/minute. Since we only fetch on track *change* (not every poll), this is well within limits. Add a simple throttle just in case:

- Max 1 request per second
- Exponential backoff on 403/429 responses

### 3.6 Discord external image URL support

When setting the activity, use the resolved URL directly:

```rust
Assets {
    large_image: "https://is1-ssl.mzstatic.com/.../512x512bb.jpg",
    large_text: "Album Name",
    small_image: "apple_music_logo",  // Uploaded asset key (fallback icon)
    small_text: "Apple Music",
}
```

> **Note:** External URLs in Rich Presence require the application to use the newer Activity API. If this doesn't work with the RPC crate, the fallback approach is to proxy the image or use Discord's asset upload API programmatically.

### Deliverables

- [ ] iTunes Search API integration returns album art URLs
- [ ] LRU cache (memory + disk) avoids redundant API calls
- [ ] Album art displays correctly in Discord Rich Presence
- [ ] Graceful fallback to default logo when art isn't found

---

## Phase 4 — System Tray UI & Settings

**Goal:** Polished menu bar experience with user-configurable settings.

### 4.1 System tray / menu bar

Tauri v2 has native system tray support. Configure:

- **Tray icon:** A small Apple Music-inspired note icon (or custom Karnyx Labs branding)
- **Tray menu items:**
  - 🎵 *Now Playing: Song — Artist* (disabled label, updates live)
  - Separator
  - ✅ Enable Rich Presence (toggle)
  - ⚙️ Settings... (opens settings window)
  - Separator
  - Quit

### 4.2 Settings window (frontend)

A minimal, clean settings panel:

| Setting | Type | Default |
|---|---|---|
| Enable on launch | Toggle | ✅ On |
| Show album art | Toggle | ✅ On |
| Show timestamps | Toggle | ✅ On |
| Display format | Dropdown | `"Song — Artist"` / `"Artist — Song"` |
| Idle behavior | Dropdown | `"Clear status"` / `"Show 'Paused'"` |
| Poll interval | Slider | 5 seconds (range: 2–15) |
| Launch at login | Toggle | ❌ Off |

Store settings in `~/.apple-music-rpc/config.json` using `serde_json`.

### 4.3 Launch at login

Use macOS `launchd` or Tauri's `autostart` plugin:

```bash
# Tauri plugin
cargo add tauri-plugin-autostart
```

This registers a Launch Agent in `~/Library/LaunchAgents/`.

### 4.4 Window behavior

- Main window is **hidden by default** (app lives in the menu bar)
- Settings window opens as a small, non-resizable panel
- Close button hides window (doesn't quit the app)
- Dock icon is hidden (`LSUIElement = true` in `Info.plist`)

### Deliverables

- [ ] System tray icon with live "Now Playing" label
- [ ] Settings window with all configurable options
- [ ] Settings persist to disk
- [ ] Launch at login works via macOS Launch Agent
- [ ] App runs as a menu bar utility (no dock icon)

---

## Phase 5 — Polish, Edge Cases & Distribution

**Goal:** Harden the app and prepare for distribution.

### 5.1 Edge cases to handle

| Scenario | Behavior |
|---|---|
| Apple Music not installed | Show tray warning; disable polling |
| Apple Music not running | Idle state; retry when app launches |
| Discord not running | Queue updates; reconnect on launch |
| Discord RPC socket lost | Exponential backoff reconnection |
| No internet (art fetch) | Use cached art or fallback asset |
| Track has no album | Use artist name for art search; fallback to logo |
| Very long track/artist names | Truncate to Discord's 128-char limit with ellipsis |
| Multiple Discord instances | Try IPC sockets 0–9 |
| Apple Music playing ads (free tier) | Detect and show "Advertisement" or clear status |
| System sleep/wake | Re-poll and re-sync on wake |

### 5.2 Logging & diagnostics

- Use the `tracing` crate for structured logging
- Log levels: `ERROR` for failures, `INFO` for state transitions, `DEBUG` for poll results
- Write logs to `~/.apple-music-rpc/logs/`
- Include a "Copy debug log" option in the tray menu

### 5.3 Code signing & notarization

Required for macOS distribution outside the App Store:

1. Obtain an Apple Developer certificate ($99/yr)
2. Configure Tauri's bundler for code signing:
   ```json
   // tauri.conf.json
   {
     "bundle": {
       "macOS": {
         "signingIdentity": "Developer ID Application: Karnyx Labs LLC",
         "notarization": {
           "teamId": "<TEAM_ID>"
         }
       }
     }
   }
   ```
3. Notarize via `xcrun notarytool` (Tauri can automate this)

### 5.4 Build & release pipeline

- **GitHub Actions** workflow:
  - Build on macOS runner
  - Code sign + notarize
  - Generate `.dmg` installer via Tauri bundler
  - Create GitHub Release with attached `.dmg`
- **Tauri's updater plugin** for auto-updates:
  ```bash
  cargo add tauri-plugin-updater
  ```

### 5.5 Distribution options

| Channel | Notes |
|---|---|
| GitHub Releases | Primary — `.dmg` download |
| Homebrew Cask | `brew install --cask apple-music-rpc` (submit a formula) |
| Project website | Optional landing page |

### Deliverables

- [ ] All edge cases handled gracefully
- [ ] Structured logging in place
- [ ] App is code-signed and notarized
- [ ] GitHub Actions CI/CD builds `.dmg` automatically
- [ ] Auto-update mechanism works

---

## Project Structure

```
apple-music-rpc/
├── src-tauri/
│   ├── src/
│   │   ├── main.rs              # Entry point, Tauri setup
│   │   ├── tray.rs              # System tray configuration
│   │   ├── commands.rs          # Tauri IPC commands
│   │   ├── apple_music.rs       # AppleScript bridge + polling
│   │   ├── discord_rpc.rs       # Discord RPC client wrapper
│   │   ├── album_art.rs         # iTunes Search API + caching
│   │   ├── config.rs            # Settings persistence
│   │   └── state.rs             # App state management
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/                          # Frontend (TypeScript)
│   ├── index.html               # Settings window
│   ├── main.ts                  # Frontend logic
│   └── styles.css               # Minimal styling
├── icons/                        # App + tray icons
├── .github/
│   └── workflows/
│       └── release.yml          # CI/CD pipeline
├── README.md
└── LICENSE
```

---

## Key Dependencies (Rust / Cargo)

| Crate | Purpose |
|---|---|
| `tauri` v2 | App framework |
| `tauri-plugin-autostart` | Launch at login |
| `tauri-plugin-updater` | Auto-updates |
| `discord-rich-presence` | Discord IPC/RPC |
| `reqwest` | HTTP client (iTunes API) |
| `serde` / `serde_json` | Serialization |
| `tokio` | Async runtime |
| `tracing` / `tracing-subscriber` | Logging |
| `lru` | In-memory LRU cache |
| `dirs` | Resolve `~/.apple-music-rpc/` path |

---

## Estimated Timeline

| Phase | Scope | Estimate |
|---|---|---|
| Phase 1 | Scaffolding + AppleScript polling | 2–3 days |
| Phase 2 | Discord RPC integration | 2–3 days |
| Phase 3 | Album art fetching + caching | 1–2 days |
| Phase 4 | System tray UI + settings | 2–3 days |
| Phase 5 | Polish + distribution | 2–3 days |
| **Total** | | **~10–14 days** |

---

## Future Enhancements (Post-MVP)

- **Song link in status** — Include an Apple Music link so friends can open the track
- **Listening history** — Log tracks to a local SQLite database for personal stats
- **Scrobbling** — Optional Last.fm integration
- **Shortcuts integration** — macOS Shortcuts actions to toggle presence
- **Windows support** — Replace AppleScript with iTunes COM automation
- **Custom themes** — Let users pick their own small icon / branding
- **"Listening With" feature** — Detect if friends are also using the app (ambitious)
