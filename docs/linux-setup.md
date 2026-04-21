# Linux Setup Guide

> Back to [Documentation Index](index.md) | [README](../README.md)

Rundale runs natively on Linux. GPU acceleration is supported via NVIDIA (CUDA) and AMD (ROCm) but is optional — CPU-only works fine with smaller models.

## Prerequisites

### 1. Install Build Essentials

Rust and Tauri require a C linker, basic build tools, and WebKit2GTK libraries.

**Ubuntu / Debian:**

```sh
sudo apt update
sudo apt install build-essential pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev \
    libappindicator3-dev librsvg2-dev patchelf
```

**Fedora / RHEL:**

```sh
sudo dnf groupinstall "Development Tools"
sudo dnf install pkg-config gtk3-devel webkit2gtk4.1-devel libappindicator-gtk3-devel \
    librsvg2-devel patchelf
```

**Arch Linux:**

```sh
sudo pacman -S base-devel pkg-config gtk3 webkit2gtk-4.1 libappindicator-gtk3 \
    librsvg patchelf
```

### 2. Install Rust

Install via [rustup](https://rustup.rs/):

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Follow the on-screen prompts (the defaults are fine). Then reload your shell:

```sh
source "$HOME/.cargo/env"
```

**Minimum Rust edition:** 2024. Run `rustup update` if you have an older toolchain.

### 3. Install Node.js

Required for the Tauri GUI frontend. Install via your package manager:

```sh
# Ubuntu/Debian (via NodeSource)
curl -fsSL https://deb.nodesource.com/setup_22.x | sudo -E bash -
sudo apt install nodejs

# Fedora
sudo dnf install nodejs

# Arch
sudo pacman -S nodejs npm
```

Or use [nvm](https://github.com/nvm-sh/nvm) for version management. Node.js v20+ recommended.

### 4. Install Tauri CLI

```sh
cargo install tauri-cli
```

### 5. Install Ollama

Install via the official script (auto-detects GPU):

```sh
curl -fsSL https://ollama.com/install.sh | sh
```

Start the Ollama service:

```sh
# If installed as a systemd service (default):
sudo systemctl start ollama

# Or run manually:
ollama serve
```

Verify it is running:

```sh
curl http://localhost:11434/api/tags
```

### 6. GPU Setup (Optional)

GPU acceleration is optional but strongly recommended for larger models.

**NVIDIA (CUDA):** Install the proprietary NVIDIA drivers for your distribution. Ollama detects CUDA automatically. Verify with `nvidia-smi`.

**AMD (ROCm):** Install ROCm following the [official guide](https://rocm.docs.amd.com/). Verify with `rocm-smi`.

**CPU-only:** No extra setup needed. Use a smaller model (`gemma4:e2b`).

### 7. Pull a Model

```sh
# 36 GB+ VRAM — dense 31B, top quality
ollama pull gemma4:31b

# 24 GB+ VRAM — MoE (4B active), fast
ollama pull gemma4:26b

# Default pick on most machines (~10 GB edge model)
ollama pull gemma4:e4b

# 8 GB or CPU — lighter edge model
ollama pull gemma4:e2b
```

See [ADR-005](adr/005-ollama-local-inference.md) for model selection details.

## Build & Run

```sh
git clone <repo-url> parish
cd parish
```

### GUI Mode (Tauri Desktop App)

```sh
# Install frontend dependencies (one-time)
cd ui && npm install && cd ..

# Launch the desktop app (Vite hot-reload + Rust backend)
cargo tauri dev
```

For a production bundle:

```sh
cargo tauri build
```

### Headless Mode

For piping input/output or running without a UI:

```sh
cargo run -- --headless
```

### Headless Screenshot Capture

To capture GUI screenshots on a headless server (e.g., CI):

```sh
# Install xvfb if not present
sudo apt install xvfb    # Ubuntu/Debian
sudo dnf install xorg-x11-server-Xvfb  # Fedora

# Capture screenshots at 4 times of day
xvfb-run -a cargo tauri dev -- -- --screenshot docs/screenshots
```

## Configuration (Optional)

Rundale works out of the box with Ollama defaults. To use an alternative LLM provider, copy the example config:

```sh
cp .env.example .env
```

Edit `.env` to set your provider, API key, and model. See [Architecture Overview](design/overview.md) for details.

## Troubleshooting

### `cargo build` fails with linker errors

Build tools or WebKit2GTK dev headers are missing. See step 1 above.

### Ollama not responding

- Check the service status: `systemctl status ollama`.
- Start it if stopped: `sudo systemctl start ollama` or run `ollama serve` manually.
- Verify the port: `curl http://localhost:11434/api/tags`.

### GUI mode fails to start

- Ensure WebKit2GTK 4.1 dev headers are installed (see step 1).
- Ensure a display server is running (X11 or Wayland).
- On a headless server, use `xvfb-run` (see screenshot capture section above).

### Model runs slowly

- Check GPU utilization with `nvidia-smi` (NVIDIA) or `rocm-smi` (AMD).
- Try a smaller model (`gemma4:e2b`) for CPU-only systems.
