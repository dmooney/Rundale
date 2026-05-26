# parish-palette — agent scope

Backend-agnostic time-of-day color interpolation for Parish. Leaf crate with a single responsibility: compute color values (sky, ambient light, foreground, background) based on the current in-game hour and minute. Minimal dependency footprint — depends only on `parish-config`. Consumed by the Svelte UI for ambient rendering. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p parish-palette                   # unit (interpolation + contrast)
cargo test -p parish-palette -- --nocapture     # with stdout
```

## Local gotchas

- **Single-file leaf crate.** All logic lives in `src/lib.rs` — no submodules, no split. Should never grow beyond a single file.
- **No new dependencies without scrutiny.** Only `parish-config` is allowed. Adding `parish-core`, `parish-types`, or any other parish crate violates the architecture-fitness test (rule #1). Adding any third-party dependency must be justified by measurable need.
- **Color values cross the IPC boundary to the Svelte UI.** `RawPalette` and `RawColor` are serialised as `ThemePalette` in `parish-core`'s IPC types. Changing the slot layout breaks the frontend contract.
- **No built-in keyframes.** The engine ships no hardcoded palettes — mods supply them via `ui.toml`'s `[[theme.keyframes]]`. `neutral_grey_palette()` is the sole fallback for the boot/empty-mod state.
- **Platform-agnostic by design.** No platform-specific code, no conditional compilation targets, no runtime feature detection. If a platform needs different colour interpolation, that belongs in the mod's `ui.toml`.
- **No world or NPC dependency.** `parish-palette` computes colour from (hour, minute, keyframes, config) alone — it must never read world state, NPC state, or any game simulation.

## Module map

`lib.rs` — all logic: `RawColor`/`RawPalette`/`Keyframe` types, hex parsing, linear interpolation, wrap-around midnight handling, luminance contrast enforcement, and the public `compute_palette_with_keyframes` entry point.
