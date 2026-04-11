# Windows Setup Guide

> Back to [Documentation Index](index.md) | [README](../README.md)

Rundale runs natively on Windows — no WSL or Docker required.

## Prerequisites

### 1. Install Rust

Install via [rustup](https://rustup.rs/). The installer will prompt you to install the MSVC build tools (Visual Studio C++ Build Tools) if they are not already present.

From PowerShell or Command Prompt:

```powershell
# After installing rustup, verify:
rustc --version
cargo --version
```

**Minimum Rust edition:** 2024. Run `rustup update` if you have an older toolchain.

### 2. Install Node.js

Required for the Tauri GUI frontend. Download from [nodejs.org](https://nodejs.org/) (v20+ LTS recommended).

Verify:

```powershell
node --version
npm --version
```

### 3. Install Tauri CLI

```powershell
cargo install tauri-cli
```

### 4. Install WebView2

Tauri uses Microsoft Edge WebView2 for rendering. It ships with Windows 11 by default. On Windows 10, download the [Evergreen Bootstrapper](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) if not already installed.

### 5. Install Ollama

Download the Windows installer from [ollama.com/download/windows](https://ollama.com/download/windows).

After installation, Ollama runs as a background service on `localhost:11434`. Verify it is running:

```powershell
curl http://localhost:11434/api/tags
```

### 6. Pull a Model

```powershell
ollama pull qwen3:14b
```

See [ADR-005](adr/005-ollama-local-inference.md) for model selection details.

## Build & Run

```powershell
git clone <repo-url> parish
cd parish
```

### GUI Mode (Tauri Desktop App)

```powershell
# Install frontend dependencies (one-time)
cd ui
npm install
cd ..

# Launch the desktop app (Vite hot-reload + Rust backend)
cargo tauri dev
```

For a production bundle:

```powershell
cargo tauri build
```

### TUI Mode (Terminal)

```powershell
cargo run
```

### Headless Mode

For piping input/output or running without a terminal UI:

```powershell
cargo run -- --headless
```

## Terminal Recommendations (TUI Mode)

Rundale uses a TUI with 24-bit true color. For the best experience:

- **Windows Terminal** (default on Windows 11, available from the Microsoft Store on Windows 10) — full true-color and Unicode support.
- **PowerShell 7+** in Windows Terminal works well.
- **Older terminals** (cmd.exe, legacy conhost) may have limited color support.

Ensure your terminal window is at least **120 columns x 40 rows** for the intended layout.

## Configuration (Optional)

Rundale works out of the box with Ollama defaults. To use an alternative LLM provider, copy the example config:

```powershell
copy .env.example .env
```

Edit `.env` to set your provider, API key, and model. See [Architecture Overview](design/overview.md) for details.

## Troubleshooting

### `cargo build` fails with linker errors

You need the MSVC C++ Build Tools. Install them via:
- The [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) standalone installer, or
- The full Visual Studio installer (select "Desktop development with C++").

### Ollama not responding

- Check that the Ollama service is running in the system tray.
- Verify the port: `curl http://localhost:11434/api/tags`.
- Firewall software may block localhost connections — add an exception if needed.

### TUI looks garbled or has no color

- Switch to Windows Terminal if you are using cmd.exe or legacy conhost.
- Ensure your terminal font supports Unicode (e.g., Cascadia Code, Consolas).

### Model runs slowly

- Check GPU utilization while the model is running.
- Try a smaller model (`qwen3:4b` or `qwen3:1.7b`) for CPU-only systems.

## Alternative: WSL

If you prefer a Linux environment, WSL 2 works fine. Install [WSL](https://learn.microsoft.com/en-us/windows/wsl/install), then follow the [Linux setup guide](linux-setup.md). This is only necessary if you have a specific preference for Linux tooling.
