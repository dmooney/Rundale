# Setup Guide

> Back to [Documentation Index](index.md) | [README](../README.md)

## Common Prerequisites (all platforms)

### Install Rust

Install via [rustup](https://rustup.rs/):

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Follow the on-screen prompts (the defaults are fine). Then reload your shell:

```sh
source "$HOME/.cargo/env"
```

**Minimum Rust edition:** 2024. Run `rustup update` if you have an older toolchain.

### Install Node.js

Required for the Tauri GUI frontend. Node.js v20+ recommended.

- **macOS:** `brew install node` or download from [nodejs.org](https://nodejs.org/)
- **Linux:** Use your package manager or [nvm](https://github.com/nvm-sh/nvm) for version management
- **Windows:** Download from [nodejs.org](https://nodejs.org/) (v20+ LTS recommended)

### Install Tauri CLI

```sh
cargo install tauri-cli
```

### Pull a Model

Rundale auto-detects your hardware at first run and picks the best **gemma4** tier. You can pre-pull a model to skip that first-run download:

```sh
# 36 GB+ (VRAM or unified memory) — dense 31B, best quality
ollama pull gemma4:31b

# 24 GB+ — Mixture-of-Experts (4B active), fast
ollama pull gemma4:26b

# Default pick on most machines (~10 GB edge model)
ollama pull gemma4:e4b

# 8 GB or CPU — lighter edge model
ollama pull gemma4:e2b
```

See [ADR-005](adr/005-ollama-local-inference.md) for model selection details.

### Configuration (Optional)

Rundale works out of the box with Ollama defaults. To use an alternative LLM provider, copy the example config:

```sh
cp .env.example .env
```

Edit `.env` to set your provider, API key, and model. See the comments in `.env.example` for options. You can also configure via `parish.toml` or CLI flags — see [Architecture Overview](design/overview.md) for details.

---

## macOS

Rundale runs natively on macOS — Intel and Apple Silicon (M1/M2/M3/M4) are both supported.

### Install Xcode Command Line Tools

Rust requires a C linker. Install Xcode Command Line Tools if you haven't already:

```sh
xcode-select --install
```

### Install Ollama

Download the macOS app from [ollama.com/download/mac](https://ollama.com/download/mac), or install via Homebrew:

```sh
brew install ollama
```

After installation, launch Ollama — it runs as a menu bar app and serves on `localhost:11434`. Verify it is running:

```sh
curl http://localhost:11434/api/tags
```

> **Note:** On Apple Silicon, Ollama uses Metal for GPU acceleration automatically — no extra drivers needed. Rundale auto-detects unified memory via `sysctl hw.memsize`.

### Build & Run

```sh
git clone <repo-url> parish
cd parish

# GUI Mode (Tauri Desktop App)
cd ui && npm install && cd ..           # one-time frontend deps
cargo tauri dev                          # Vite hot-reload + Rust backend

# Production bundle
cargo tauri build

# Headless Mode (stdin/stdout REPL)
cargo run -- --headless
```

### Troubleshooting

**`cargo build` fails with "xcrun: error":**
Xcode Command Line Tools are missing or need updating:

```sh
xcode-select --install
# Or if already installed but broken:
sudo xcode-select --reset
```

**Ollama not responding:**
- Ensure the Ollama app is running (check the menu bar icon).
- Verify the port: `curl http://localhost:11434/api/tags`.
- If you installed via Homebrew, start the service: `brew services start ollama`.

**Model runs slowly:**
- On Apple Silicon, ensure Ollama is using Metal (it should by default). Check with `ollama ps` to see GPU utilization.
- Close other memory-intensive applications — the model needs free unified memory.
- Try a smaller model (`gemma4:e2b`) if performance is poor.

---

## Linux

Rundale runs natively on Linux. GPU acceleration is supported via NVIDIA (CUDA) and AMD (ROCm) but is optional — CPU-only works fine with smaller models.

### Install Build Essentials

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

### Install Ollama

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

### GPU Setup (Optional)

GPU acceleration is optional but strongly recommended for larger models.

- **NVIDIA (CUDA):** Install the proprietary NVIDIA drivers for your distribution. Ollama detects CUDA automatically. Verify with `nvidia-smi`.
- **AMD (ROCm):** Install ROCm following the [official guide](https://rocm.docs.amd.com/). Verify with `rocm-smi`.
- **CPU-only:** No extra setup needed. Use a smaller model (`gemma4:e2b`).

### Build & Run

```sh
git clone <repo-url> parish
cd parish

# GUI Mode (Tauri Desktop App)
cd ui && npm install && cd ..           # one-time frontend deps
cargo tauri dev                          # Vite hot-reload + Rust backend

# Production bundle
cargo tauri build

# Headless Mode (stdin/stdout REPL)
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

### Troubleshooting

**`cargo build` fails with linker errors:**
Build tools or WebKit2GTK dev headers are missing. See the Build Essentials section above.

**Ollama not responding:**
- Check the service status: `systemctl status ollama`.
- Start it if stopped: `sudo systemctl start ollama` or run `ollama serve` manually.
- Verify the port: `curl http://localhost:11434/api/tags`.

**GUI mode fails to start:**
- Ensure WebKit2GTK 4.1 dev headers are installed (see Build Essentials).
- Ensure a display server is running (X11 or Wayland).
- On a headless server, use `xvfb-run` (see screenshot capture section above).

**Model runs slowly:**
- Check GPU utilization with `nvidia-smi` (NVIDIA) or `rocm-smi` (AMD).
- Try a smaller model (`gemma4:e2b`) for CPU-only systems.

---

## Windows

Rundale runs natively on Windows — no WSL or Docker required.

### Install Rust

Install via [rustup](https://rustup.rs/). The installer will prompt you to install the MSVC build tools (Visual Studio C++ Build Tools) if they are not already present.

From PowerShell or Command Prompt:

```powershell
# After installing rustup, verify:
rustc --version
cargo --version
```

**Minimum Rust edition:** 2024. Run `rustup update` if you have an older toolchain.

### Install WebView2

Tauri uses Microsoft Edge WebView2 for rendering. It ships with Windows 11 by default. On Windows 10, download the [Evergreen Bootstrapper](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) if not already installed.

### Install Ollama

Download the Windows installer from [ollama.com/download/windows](https://ollama.com/download/windows).

After installation, Ollama runs as a background service on `localhost:11434`. Verify it is running:

```powershell
curl http://localhost:11434/api/tags
```

### Build & Run

```powershell
git clone <repo-url> parish
cd parish

# GUI Mode (Tauri Desktop App)
cd ui
npm install
cd ..

# Launch the desktop app (Vite hot-reload + Rust backend)
cargo tauri dev

# Production bundle
cargo tauri build

# Headless Mode
cargo run -- --headless
```

### Troubleshooting

**`cargo build` fails with linker errors:**
You need the MSVC C++ Build Tools. Install them via:
- The [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) standalone installer, or
- The full Visual Studio installer (select "Desktop development with C++").

**Ollama not responding:**
- Check that the Ollama service is running in the system tray.
- Verify the port: `curl http://localhost:11434/api/tags`.
- Firewall software may block localhost connections — add an exception if needed.

**Model runs slowly:**
- Check GPU utilization while the model is running.
- Try a smaller model (`gemma4:e2b`) for CPU-only systems.

### Alternative: WSL

If you prefer a Linux environment, WSL 2 works fine. Install [WSL](https://learn.microsoft.com/en-us/windows/wsl/), then follow the Linux section above. This is only necessary if you have a specific preference for Linux tooling.
