# Windows Setup Guide

> Back to [Documentation Index](index.md) | [README](../README.md)

Parish runs natively on Windows — no WSL or Docker required. All dependencies (crossterm, bundled SQLite, tokio, reqwest) are fully cross-platform.

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

### 2. Install Ollama

Download the Windows installer from [ollama.com/download/windows](https://ollama.com/download/windows).

After installation, Ollama runs as a background service on `localhost:11434`. Verify it is running:

```powershell
curl http://localhost:11434/api/tags
```

Or open `http://localhost:11434` in a browser — you should see "Ollama is running".

### 3. Pull a Model

Parish uses Ollama for NPC inference. Pull a model before running:

```powershell
ollama pull llama3.2
```

See [ADR-005](adr/005-ollama-local-inference.md) for model selection details.

## Build & Run

```powershell
git clone <repo-url> parish
cd parish
cargo build
cargo run
```

For an optimized build:

```powershell
cargo build --release
cargo run --release
```

## Terminal Recommendations

Parish uses a TUI (terminal user interface) with 24-bit true color. For the best experience:

- **Windows Terminal** (default on Windows 11, available from the Microsoft Store on Windows 10) — full true-color and Unicode support.
- **PowerShell 7+** in Windows Terminal works well.
- **Older terminals** (cmd.exe, legacy conhost) may have limited color support. The TUI will still function but colors may be degraded.

Ensure your terminal window is at least **120 columns x 40 rows** for the intended layout.

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

### SQLite errors

Parish bundles SQLite via `rusqlite` with `features = ["bundled"]`, so no system SQLite installation is needed. If you see SQLite-related build errors, ensure your MSVC toolchain is properly installed (see linker errors above).

## Alternative: WSL

If you prefer a Linux environment, WSL 2 works fine. Install [WSL](https://learn.microsoft.com/en-us/windows/wsl/install), then follow the standard Linux setup (install Rust via rustup, install Ollama for Linux). This is only necessary if you have a specific preference for Linux tooling — there is no technical advantage for this project.

## Alternative: Docker

Not recommended. The TUI requires direct terminal access, which is awkward through Docker. There is no Dockerfile provided.
