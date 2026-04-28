# parish-palette

Backend-agnostic time-of-day color interpolation for the Parish engine.

Provides smooth RGB palette computation that interpolates between time-of-day keyframes and enforces a minimum foreground/background contrast floor. UI renderers (Tauri, web server, headless logging) consume `RawPalette` values from this crate.

## Why a sibling crate

The palette logic is presentation-layer infrastructure shared by every UI surface. It depends only on `parish-config` (for `PaletteConfig`), and has no game-state dependencies. Keeping it as a sibling crate (rather than a module of `parish-world`) signals that it is *not* world state — it's a derived view of world state used by renderers.

## Pipeline

```
parish-config::PaletteConfig (tuning) ──► parish-palette::compute_palette() ──► RawPalette
                                                                                    │
                                                                                    ▼
                                                              parish-core::ipc::types::ThemePalette
                                                              (CSS-hex wire format → frontend)
```

The `From<RawPalette> for ThemePalette` impl lives in the IPC types module; this crate stays free of any wire format.
