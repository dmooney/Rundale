# macOS Setup Guide

> Back to [Documentation Index](index.md) | [README](../README.md)

Rundale runs natively on macOS — Intel and Apple Silicon (M1/M2/M3/M4) are both supported.

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

**Minimum Rust edition:** 2024. Run `rustup update` if you have an older toolchain.

### 3. Install Node.js

Required for the Tauri GUI frontend. Install via Homebrew:

```sh
brew install node
```

Or download from [nodejs.org](https://nodejs.org/) (v20+ recommended).

### 4. Install Tauri CLI

```sh
cargo install tauri-cli
```

### 5. Install Ollama

Download the macOS app from [ollama.com/download/mac](https://ollama.com/download/mac), or install via Homebrew:

```sh
brew install ollama
```

After installation, launch Ollama — it runs as a menu bar app and serves on `localhost:11434`. Verify it is running:

```sh
curl http://localhost:11434/api/tags
```

> **Note:** On Apple Silicon, Ollama uses Metal for GPU acceleration automatically — no extra drivers needed.

### 6. Pull a Model

Rundale auto-detects your hardware (unified memory on Apple Silicon via `sysctl hw.memsize`) and picks the best **gemma4** tier when you first run the game. You can pre-pull a model to skip that first-run download:

```sh
# 36 GB+ unified memory — dense 31B, best quality
ollama pull gemma4:31b

# 24 GB+ — Mixture-of-Experts (4B active), fast
ollama pull gemma4:26b

# 16 GB (default for most Macs) — edge 4.5B effective
ollama pull gemma4:e4b

# 8 GB / older Mac — edge 2.3B effective
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

## Configuration (Optional)

Rundale works out of the box with Ollama defaults. To use an alternative LLM provider, copy the example config:

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

### Model runs slowly

- On Apple Silicon, ensure Ollama is using Metal (it should by default). Check with `ollama ps` to see GPU utilization.
- Close other memory-intensive applications — the model needs free unified memory.
- Try a smaller model (`gemma4:e2b`) if performance is poor.
