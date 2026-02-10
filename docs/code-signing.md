# Code Signing & Release Setup

This guide covers how to configure Apple code signing, notarization, and Tauri updater keys for automated releases.

## 1. Apple Developer Certificate

### Create a Certificate Signing Request (CSR)

1. Open **Keychain Access** on your Mac
2. From the menu bar: **Keychain Access → Certificate Assistant → Request a Certificate From a Certificate Authority...**
3. Enter your email address and your name
4. Leave "CA Email Address" blank
5. Select **Saved to disk** and click Continue
6. Save the `.certSigningRequest` file somewhere you can find it

### Create the Developer ID Application Certificate

1. Go to [developer.apple.com](https://developer.apple.com) → **Account** → **Certificates, Identifiers & Profiles**
2. Click the **+** button next to "Certificates"
3. Under "Software", select **Developer ID Application** and click Continue
4. Upload the `.certSigningRequest` file you created above and click Continue
5. Download the resulting `.cer` file
6. Double-click the `.cer` file to install it in Keychain Access

## 2. Export the Certificate

1. Open **Keychain Access**
2. In the left sidebar, select the **login** keychain and the **My Certificates** category
3. Find "Developer ID Application: Your Name" — expand it to confirm a private key is attached underneath
4. Select the certificate (not the key), right-click → **Export Items...**
5. Choose **Personal Information Exchange (.p12)** from the format dropdown, then save and set a password

> **Note:** If the .p12 option is grayed out, the private key isn't paired with the certificate. This happens if the certificate was installed on a different Mac than where the CSR was created. You must export from the same Mac/keychain where you generated the CSR in step 1.

## 3. Base64 Encode the Certificate

```bash
base64 -i cert.p12 | pbcopy
```

This copies the base64 string to your clipboard for use as a GitHub secret.

## 4. App-Specific Password

1. Go to [appleid.apple.com](https://appleid.apple.com) → Sign-In and Security → App-Specific Passwords
2. Generate a new password for "AMDP Notarization"

## 5. Find Your Team ID

1. Go to [developer.apple.com](https://developer.apple.com) → Account → Membership Details
2. Copy the 10-character **Team ID**

## 6. Generate Tauri Updater Keys

```bash
npx @tauri-apps/cli signer generate -w ~/.tauri/amdp.key
```

This creates a keypair. The **public key** goes in `tauri.conf.json` under `plugins.updater.pubkey`. The **private key** and its password become GitHub secrets.

## 7. GitHub Secrets

Configure the following secrets in your repository settings (Settings → Secrets and variables → Actions):

| Secret | Value |
|--------|-------|
| `APPLE_CERTIFICATE` | Base64-encoded .p12 Developer ID Application certificate |
| `APPLE_CERTIFICATE_PASSWORD` | Password used when exporting the .p12 file |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | Your Apple ID email used for notarization |
| `APPLE_PASSWORD` | App-specific password from step 4 |
| `APPLE_TEAM_ID` | 10-character Team ID from step 5 |
| `TAURI_SIGNING_PRIVATE_KEY` | Contents of `~/.tauri/amdp.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password from `signer generate` |

## 8. Update pubkey in tauri.conf.json

After running `signer generate`, paste the **public key** into `src-tauri/tauri.conf.json`:

```json
"plugins": {
  "updater": {
    "endpoints": ["https://github.com/Kassicus/AMDP/releases/latest/download/latest.json"],
    "pubkey": "YOUR_PUBLIC_KEY_HERE"
  }
}
```

## 9. Creating a Release

Tag and push to trigger the release workflow:

```bash
git tag v0.2.0
git push origin v0.2.0
```

This creates a **draft** GitHub Release with the `.dmg` and `latest.json` attached. Review and publish the draft to make it available for auto-updates.
