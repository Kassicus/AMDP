use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const MAX_MEMORY_ENTRIES: usize = 500;
const DISK_TTL_SECS: u64 = 30 * 24 * 60 * 60; // 30 days
const MIN_REQUEST_INTERVAL_MS: u64 = 1000;

// --- Disk cache ---

#[derive(Serialize, Deserialize)]
struct DiskCacheEntry {
    url: String,
    fetched_at: u64,
}

#[derive(Serialize, Deserialize, Default)]
struct DiskCache {
    entries: HashMap<String, DiskCacheEntry>,
}

// --- Memory cache ---

struct MemoryCacheEntry {
    url: String,
    inserted_at: Instant,
}

// --- iTunes API response ---

#[derive(Deserialize)]
struct ItunesSearchResponse {
    results: Vec<ItunesResult>,
}

#[derive(Deserialize)]
struct ItunesResult {
    #[serde(rename = "artworkUrl100")]
    artwork_url_100: Option<String>,
}

// --- Resolver ---

pub struct AlbumArtResolver {
    memory_cache: HashMap<String, MemoryCacheEntry>,
    disk_cache: DiskCache,
    disk_cache_dirty: bool,
    disk_cache_path: PathBuf,
    client: reqwest::Client,
    last_request_at: Option<Instant>,
}

fn cache_key(artist: &str, album: &str) -> String {
    let artist_clean = artist.to_lowercase().trim().to_string();
    let album_clean = album.to_lowercase().trim().to_string();
    if album_clean.is_empty() {
        artist_clean
    } else {
        format!("{artist_clean}::{album_clean}")
    }
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for byte in s.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push(char::from(b"0123456789ABCDEF"[(byte >> 4) as usize]));
                out.push(char::from(b"0123456789ABCDEF"[(byte & 0x0F) as usize]));
            }
        }
    }
    out
}

impl AlbumArtResolver {
    pub fn new() -> Self {
        let disk_cache_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".amdp")
            .join("art-cache.json");

        let disk_cache = Self::load_disk_cache(&disk_cache_path);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            memory_cache: HashMap::new(),
            disk_cache,
            disk_cache_dirty: false,
            disk_cache_path,
            client,
            last_request_at: None,
        }
    }

    fn load_disk_cache(path: &PathBuf) -> DiskCache {
        let data = match std::fs::read_to_string(path) {
            Ok(d) => d,
            Err(_) => return DiskCache::default(),
        };

        let mut cache: DiskCache = match serde_json::from_str(&data) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to parse art cache: {e}");
                return DiskCache::default();
            }
        };

        // Prune expired entries
        let now = now_unix_secs();
        cache.entries.retain(|_, entry| {
            now.saturating_sub(entry.fetched_at) < DISK_TTL_SECS
        });

        cache
    }

    pub async fn resolve(&mut self, artist: &str, album: &str) -> Option<String> {
        let key = cache_key(artist, album);

        // 1. Memory cache
        if let Some(entry) = self.memory_cache.get(&key) {
            tracing::debug!("Art cache hit (memory): {key}");
            return Some(entry.url.clone());
        }

        // 2. Disk cache
        if let Some(entry) = self.disk_cache.entries.get(&key) {
            let now = now_unix_secs();
            if now.saturating_sub(entry.fetched_at) < DISK_TTL_SECS {
                let url = entry.url.clone();
                tracing::debug!("Art cache hit (disk): {key}");
                self.insert_memory_cache(key, url.clone());
                return Some(url);
            }
        }

        // 3. Fetch from iTunes
        let url = self.fetch_from_itunes(artist, album).await?;
        self.insert_memory_cache(key.clone(), url.clone());
        self.insert_disk_cache(key, url.clone());
        self.save_disk_cache_if_dirty();
        Some(url)
    }

    async fn fetch_from_itunes(&mut self, artist: &str, album: &str) -> Option<String> {
        self.enforce_rate_limit().await;

        let album_trimmed = album.trim();
        let query = if album_trimmed.is_empty() {
            artist.to_string()
        } else {
            format!("{} {}", artist, album_trimmed)
        };
        let url = format!(
            "https://itunes.apple.com/search?term={}&media=music&entity=album&limit=1",
            urlencode(&query)
        );

        tracing::info!("Fetching album art from iTunes: {url}");

        let resp = match self.client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("iTunes API request failed: {e}");
                return None;
            }
        };

        let body: ItunesSearchResponse = match resp.json().await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("iTunes API response parse failed: {e}");
                return None;
            }
        };

        let artwork_url = body.results.first()?.artwork_url_100.as_ref()?;

        // Upscale from 100x100 to 512x512
        let hires = artwork_url.replace("100x100bb", "512x512bb");
        Some(hires)
    }

    async fn enforce_rate_limit(&mut self) {
        if let Some(last) = self.last_request_at {
            let elapsed = last.elapsed().as_millis() as u64;
            if elapsed < MIN_REQUEST_INTERVAL_MS {
                let wait = MIN_REQUEST_INTERVAL_MS - elapsed;
                tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
            }
        }
        self.last_request_at = Some(Instant::now());
    }

    fn insert_memory_cache(&mut self, key: String, url: String) {
        if self.memory_cache.len() >= MAX_MEMORY_ENTRIES {
            // Evict oldest entry
            if let Some(oldest_key) = self
                .memory_cache
                .iter()
                .min_by_key(|(_, v)| v.inserted_at)
                .map(|(k, _)| k.clone())
            {
                self.memory_cache.remove(&oldest_key);
            }
        }
        self.memory_cache.insert(
            key,
            MemoryCacheEntry {
                url,
                inserted_at: Instant::now(),
            },
        );
    }

    fn insert_disk_cache(&mut self, key: String, url: String) {
        self.disk_cache.entries.insert(
            key,
            DiskCacheEntry {
                url,
                fetched_at: now_unix_secs(),
            },
        );
        self.disk_cache_dirty = true;
    }

    fn save_disk_cache_if_dirty(&mut self) {
        if !self.disk_cache_dirty {
            return;
        }

        if let Some(parent) = self.disk_cache_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!("Failed to create cache dir: {e}");
                return;
            }
        }

        match serde_json::to_string_pretty(&self.disk_cache) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&self.disk_cache_path, json) {
                    tracing::warn!("Failed to write art cache: {e}");
                } else {
                    self.disk_cache_dirty = false;
                    tracing::debug!("Art cache saved to {}", self.disk_cache_path.display());
                }
            }
            Err(e) => tracing::warn!("Failed to serialize art cache: {e}"),
        }
    }
}
