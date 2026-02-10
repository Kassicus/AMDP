# Discord Developer Setup

AMDP uses Discord Rich Presence to show your currently playing Apple Music track as a "Listening to" status. This requires a Discord Developer Application.

## 1. Create a Discord Application

1. Go to the [Discord Developer Portal](https://discord.com/developers/applications)
2. Click **New Application**
3. Name it **Apple Music** — this name appears as "Listening to **Apple Music**" in your Discord status
4. Accept the Terms of Service and click **Create**

## 2. Copy the Application ID

1. On the **General Information** page, find the **Application ID**
2. Copy it
3. Open `src-tauri/src/discord_rpc.rs` and replace the placeholder:
   ```rust
   const DISCORD_APP_ID: &str = "REPLACE_WITH_YOUR_APPLICATION_ID";
   ```
   with your actual Application ID:
   ```rust
   const DISCORD_APP_ID: &str = "123456789012345678";
   ```

## 3. Upload a Rich Presence Asset

1. In the Developer Portal, select your application
2. Navigate to **Rich Presence** > **Art Assets**
3. Click **Add Image(s)**
4. Upload an Apple Music logo image (recommended: 512x512 PNG)
5. Set the asset key to `apple_music_logo` (this must match exactly)
6. Click **Save Changes**

> Note: It can take a few minutes for newly uploaded assets to become available.

## 4. Verify Setup

1. Make sure Discord is running on your machine
2. Run AMDP with `npm run tauri dev`
3. Play a song in Apple Music
4. The AMDP window should show Discord status as "Connected"
5. Your Discord profile should display "Listening to Apple Music" with the track info

## Troubleshooting

**Status stays "Disconnected"**
- Ensure Discord is running (not just the browser version — the desktop app is required)
- Verify the Application ID is correct in `discord_rpc.rs`
- AMDP retries the connection automatically; wait up to 30 seconds

**Status shows "Error"**
- Check that your Application ID is valid
- Restart Discord and AMDP

**No image appears in Discord**
- Confirm the asset key is exactly `apple_music_logo`
- Wait a few minutes after uploading — Discord caches assets

**"Listening to" not appearing for friends**
- In Discord Settings > Activity Privacy, ensure "Display current activity as a status message" is enabled
