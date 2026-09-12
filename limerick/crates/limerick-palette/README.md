# limerick-palette

Backend-agnostic time-of-day color interpolation for the Limerick engine.

Provides smooth RGB palette computation that interpolates between time-of-day keyframes and enforces a minimum foreground/background contrast floor. UI renderers (Tauri, web server, headless logging) consume `RawPalette` values from this crate.

## Why a sibling crate

The palette logic is presentation-layer infrastructure shared by every UI surface. It depends only on `limerick-config` (for `PaletteConfig`), and has no game-state dependencies. Keeping it as a sibling crate (rather than a module of `limerick-world`) signals that it is _not_ world state — it's a derived view of world state used by renderers.

## Pipeline

```text
limerick-config::PaletteConfig (tuning) ──► limerick-palette::compute_palette()
                                                     │
                                                     ▼
                                          limerick-palette::compute_palette_with_config()
                                                     │
                                                     ▼
                                               RawPalette
                                                     │
                                                     ▼
                               limerick-core::ipc::types::ThemePalette
                               (CSS-hex wire format → frontend)
```

The `From<RawPalette> for ThemePalette` impl lives in the IPC types module; this crate stays free of any wire format.
