# parish-palette — agent scope

Backend-agnostic time-of-day color interpolation for Parish. Computes color values (sky, ambient light, foreground, background) from in-game hour + minute. Depends only on `parish-config`; consumed by the Svelte UI for ambient rendering. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p parish-palette                   # unit (interpolation + contrast)
cargo test -p parish-palette -- --nocapture     # with stdout
```

## Local gotchas

- **Single-file leaf crate.** All logic lives in `src/lib.rs` — no submodules, no split. Should never grow beyond a single file.
- **Only `parish-config` as a dependency (rule #1).** Adding any parish crate (e.g. `parish-core`, `parish-types`) violates the architecture-fitness test. Third-party additions require measurable justification.
- **`RawPalette`/`RawColor` cross the IPC boundary.** Serialised as `ThemePalette` in `parish-core`'s IPC types — changing the slot layout breaks the frontend contract.
- **No built-in keyframes.** Mods supply palettes via `ui.toml`'s `[[theme.keyframes]]`; `neutral_grey_palette()` is the sole boot/empty-mod fallback.
- **Platform-agnostic — no conditional compilation.** Differing color behavior belongs in the mod's `ui.toml`, not platform guards.
- **No world or NPC dependency.** Computes from (hour, minute, keyframes, config) only — never reads world or simulation state.

## Module map

`lib.rs` — all logic: `RawColor`/`RawPalette`/`Keyframe` types, hex parsing, linear interpolation, wrap-around midnight handling, luminance contrast enforcement, and the public `compute_palette_with_keyframes` entry point.
