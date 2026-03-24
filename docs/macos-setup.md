# macOS Setup Guide

> Back to [Documentation Index](index.md) | [README](../README.md)

Parish runs natively on macOS — Intel and Apple Silicon (M1/M2/M3/M4) are both supported. All dependencies (crossterm, bundled SQLite, tokio, reqwest) are fully cross-platform.

## Prerequisites

### 1. Install Xcode Command Line Tools

Rust requires a C linker. Install Xcode Command Line Tools if you haven't already:

```sh
xcode-select --install
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

Download the macOS app from [ollama.com/download/mac](https://ollama.com/download/mac), or install via Homebrew:

```sh
brew install ollama
```

After installation, launch Ollama — it runs as a menu bar app and serves on `localhost:11434`. Verify it is running:

```sh
curl http://localhost:11434/api/tags
```

Or open `http://localhost:11434` in a browser — you should see "Ollama is running".

> **Note:** On Apple Silicon, Ollama uses Metal for GPU acceleration automatically — no extra drivers needed.

### 4. Pull a Model

Parish auto-detects your hardware and selects a model when you first run the game, but you can pre-pull one:

```sh
# Apple Silicon with 16 GB+ unified memory — full quality
ollama pull qwen3:14b

# Apple Silicon with 8 GB — good quality
ollama pull qwen3:8b

# Older Mac or limited memory — lighter model
ollama pull qwen3:4b
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

Parish includes a windowed GUI mode using egui:

```sh
cargo run -- --gui
```

### Headless Mode

For piping input/output or running without a terminal UI:

```sh
cargo run -- --headless
```

## Terminal Recommendations

Parish uses a TUI (terminal user interface) with 24-bit true color. For the best experience:

- **iTerm2** — full true-color and Unicode support, highly recommended.
- **kitty** — fast GPU-accelerated terminal with excellent color support.
- **Terminal.app** — works, but verify 24-bit color is enabled (Preferences → Profiles → check "Use bright colors for bold text" is unchecked; ensure a modern color profile).

Ensure your terminal window is at least **120 columns x 40 rows** for the intended layout.

## Configuration (Optional)

Parish works out of the box with Ollama defaults. To use an alternative LLM provider, copy the example config:

```sh
cp .env.example .env
```

Edit `.env` to set your provider, API key, and model. See the comments in `.env.example` for options. You can also configure via `parish.toml` or CLI flags — see [Architecture Overview](design/overview.md) for details.

## Troubleshooting

### `cargo build` fails with "xcrun: error"

Xcode Command Line Tools are missing or need updating:

```sh
xcode-select --install
# Or if already installed but broken:
sudo xcode-select --reset
```

### Ollama not responding

- Ensure the Ollama app is running (check the menu bar icon).
- Verify the port: `curl http://localhost:11434/api/tags`.
- If you installed via Homebrew, start the service: `brew services start ollama`.

### TUI looks garbled or has no color

- Switch to iTerm2 or kitty if using Terminal.app.
- Ensure your terminal supports 24-bit true color. Test with: `printf "\x1b[38;2;255;100;0mTRUECOLOR\x1b[0m\n"` — you should see orange text.
- Resize your terminal to at least 120×40.

### SQLite errors

Parish bundles SQLite via `rusqlite` with `features = ["bundled"]`, so no system SQLite installation is needed. Build errors related to SQLite typically indicate a missing C compiler — ensure Xcode Command Line Tools are installed (see above).

### Model runs slowly

- On Apple Silicon, ensure Ollama is using Metal (it should by default). Check with `ollama ps` to see GPU utilization.
- Close other memory-intensive applications — the model needs free unified memory.
- Try a smaller model (`qwen3:4b` or `qwen3:1.7b`) if performance is poor.
