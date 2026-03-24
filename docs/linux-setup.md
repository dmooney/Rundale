# Linux Setup Guide

> Back to [Documentation Index](index.md) | [README](../README.md)

Parish runs natively on Linux. All dependencies (crossterm, bundled SQLite, tokio, reqwest) are fully cross-platform. GPU acceleration is supported via NVIDIA (CUDA) and AMD (ROCm) but is optional — CPU-only works fine with smaller models.

## Prerequisites

### 1. Install Build Essentials

Rust requires a C linker and basic build tools. Install them for your distribution:

**Ubuntu / Debian:**

```sh
sudo apt update
sudo apt install build-essential pkg-config
```

**Fedora / RHEL:**

```sh
sudo dnf groupinstall "Development Tools"
sudo dnf install pkg-config
```

**Arch Linux:**

```sh
sudo pacman -S base-devel pkg-config
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

Verify the installation:

```sh
rustc --version
cargo --version
```

**Minimum Rust edition:** 2024. Run `rustup update` if you have an older toolchain.

### 3. Install Ollama

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

Or open `http://localhost:11434` in a browser — you should see "Ollama is running".

> **Note:** Parish can also auto-install Ollama on Linux if it is not found. On first run, it will prompt to download and install it for you.

### 4. GPU Setup (Optional)

GPU acceleration is optional but strongly recommended for larger models.

**NVIDIA (CUDA):**

Install the proprietary NVIDIA drivers for your distribution. Ollama detects CUDA automatically. Verify with:

```sh
nvidia-smi
```

**AMD (ROCm):**

Install ROCm following the [official guide](https://rocm.docs.amd.com/). Verify with:

```sh
rocm-smi
```

**CPU-only:**

No extra setup needed. Use a smaller model (`qwen3:4b` or `qwen3:1.7b`) for reasonable performance.

### 5. Pull a Model

Parish auto-detects your hardware and selects a model when you first run the game, but you can pre-pull one:

```sh
# 12 GB+ VRAM — full quality
ollama pull qwen3:14b

# 6 GB+ VRAM — good quality
ollama pull qwen3:8b

# 3 GB+ VRAM or CPU — lighter model
ollama pull qwen3:4b

# CPU-only / low memory — minimal
ollama pull qwen3:1.7b
```

See [ADR-005](adr/005-ollama-local-inference.md) for model selection details.

## Build & Run

```sh
git clone <repo-url> parish
cd parish
cargo build
cargo run
```

For an optimized build:

```sh
cargo build --release
cargo run --release
```

### GUI Mode

Parish includes a windowed GUI mode using egui. This requires a display server (X11 or Wayland):

```sh
cargo run -- --gui
```

### Headless Mode

For piping input/output or running without a terminal UI:

```sh
cargo run -- --headless
```

### Headless Screenshot Capture

To capture GUI screenshots on a headless server (e.g., CI):

```sh
# Install xvfb if not present
sudo apt install xvfb    # Ubuntu/Debian
sudo dnf install xorg-x11-server-Xvfb  # Fedora

# Capture screenshots
xvfb-run -a cargo run -- --screenshot docs/screenshots
```

## Terminal Recommendations

Parish uses a TUI (terminal user interface) with 24-bit true color. For the best experience:

- **GNOME Terminal** — full true-color support, default on many distros.
- **Konsole** — KDE's terminal, excellent color and Unicode support.
- **kitty** — fast GPU-accelerated terminal with full 24-bit color.
- **Alacritty** — GPU-accelerated, minimal, full true-color support.

Ensure your terminal window is at least **120 columns x 40 rows** for the intended layout.

Test 24-bit color support with:

```sh
printf "\x1b[38;2;255;100;0mTRUECOLOR\x1b[0m\n"
```

You should see orange text. If not, upgrade your terminal or set `COLORTERM=truecolor` in your shell profile.

## Configuration (Optional)

Parish works out of the box with Ollama defaults. To use an alternative LLM provider, copy the example config:

```sh
cp .env.example .env
```

Edit `.env` to set your provider, API key, and model. See the comments in `.env.example` for options. You can also configure via `parish.toml` or CLI flags — see [Architecture Overview](design/overview.md) for details.

## Troubleshooting

### `cargo build` fails with linker errors

Build tools are missing. Install them:

```sh
# Ubuntu/Debian
sudo apt install build-essential pkg-config

# Fedora
sudo dnf groupinstall "Development Tools"
```

### Ollama not responding

- Check the service status: `systemctl status ollama`.
- Start it if stopped: `sudo systemctl start ollama` or run `ollama serve` manually.
- Verify the port: `curl http://localhost:11434/api/tags`.
- Check for port conflicts: `ss -tlnp | grep 11434`.

### GPU not detected by Ollama

- **NVIDIA:** Ensure `nvidia-smi` works and shows your GPU. Install drivers via your distro's package manager or the NVIDIA `.run` installer.
- **AMD:** Ensure `rocm-smi` works. ROCm requires specific kernel versions — check the [compatibility matrix](https://rocm.docs.amd.com/).
- After installing drivers, restart Ollama: `sudo systemctl restart ollama`.

### TUI looks garbled or has no color

- Upgrade to a modern terminal (GNOME Terminal, kitty, Alacritty).
- Set `COLORTERM=truecolor` in your `.bashrc` or `.zshrc`.
- Ensure your `TERM` variable is set to a value supporting 256+ colors (e.g., `xterm-256color`).
- Resize your terminal to at least 120×40.

### GUI mode fails to start

- Ensure a display server is running (X11 or Wayland).
- On Wayland, you may need to set `WAYLAND_DISPLAY` or `DISPLAY` environment variables.
- On a headless server, use `xvfb-run` (see screenshot capture section above).

### SQLite errors

Parish bundles SQLite via `rusqlite` with `features = ["bundled"]`, so no system SQLite installation is needed. Build errors related to SQLite typically indicate a missing C compiler — ensure build-essential or equivalent is installed.

### Model runs slowly

- Check GPU utilization with `nvidia-smi` (NVIDIA) or `rocm-smi` (AMD) while the model is running.
- If the model falls back to CPU, ensure Ollama has GPU access (check `ollama ps`).
- Try a smaller model (`qwen3:4b` or `qwen3:1.7b`) for CPU-only systems.
- Ensure no other processes are consuming GPU memory.
