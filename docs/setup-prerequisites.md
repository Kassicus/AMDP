# Setup Prerequisites

## Rust Toolchain

AMDP is built with [Tauri v2](https://v2.tauri.app/), which requires a Rust toolchain.

### Install Rust via rustup

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Choose the default installation (option 1). After installation, restart your terminal or run:

```bash
source "$HOME/.cargo/env"
```

Verify the installation:

```bash
rustc --version
cargo --version
```

## Xcode Command Line Tools

Tauri on macOS requires Xcode CLI tools for compilation.

```bash
xcode-select --install
```

If already installed, this will print an error — that's fine.

## Node.js

Node.js v18+ is required for the frontend build toolchain.

```bash
node --version   # should be v18+
npm --version
```

## Next Steps

After installing Rust, return to the project root and run:

```bash
npm install
npm run tauri dev
```

The first build will take 3-5 minutes as Cargo downloads and compiles all Rust dependencies.

On first launch, macOS will prompt you to grant AMDP permission to control Music.app via System Events. This is required for the AppleScript bridge to detect playback state.
