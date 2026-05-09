# parish-palette — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-008 | Dead/Unused Export | P3 | `src/lib.rs:296` | `compute_palette_with_config` is `pub` but has no callers outside this crate — only the in-crate `compute_palette` wrapper and unit tests use it. Either expose it via README/consumer wiring or drop `pub`. |
| TD-009 | Duplication | P3 | `src/lib.rs:157, 266, 271-273` | The `v.round().clamp(0.0, 255.0) as u8` cast pattern is written four times (`lerp_u8`, the gray fallback in `ensure_color_contrast`, plus three channel scales). Extract a `f32_to_u8_clamped` helper. |
| TD-010 | Redundant Test | P3 | `src/lib.rs:523-527` | `test_compute_palette_produces_valid_colors` only asserts `bg != black` at one timestamp; fully subsumed by `test_compute_palette_all_hours_valid` (line 530). Delete to reduce noise. |
| TD-011 | Stale Docs | P3 | `README.md:14` | Pipeline diagram only mentions `compute_palette()`; the sibling public entry point `compute_palette_with_config(hour, minute, &PaletteConfig)` is not documented. Either document it or address via TD-008. |
| TD-012 | Brittle Conditional | P3 | `src/lib.rs:224-225` | `interpolated_palette` ends with a silent `KEYFRAMES[0].palette` fallback labelled "shouldn't be reached". A future bug in the search loop would yield a Midnight palette instead of failing loudly — switch to `unreachable!()` (or `debug_assert!` + documented behaviour) so regressions surface in tests. |

## In Progress

*(none)*

## Done

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Stale Docs | P2 | `src/lib.rs:30` | Fixed doc comment: replaced `crate::gui::theme::GuiPalette` with `parish_core::ipc::types::ThemePalette`. |
| TD-002 | Stale Docs | P2 | `src/lib.rs:58` | Fixed doc comment: removed stale `TimeOfDay` reference, now describes time-of-day periods in plain language. |
| TD-003 | Weak Tests | P2 | `src/lib.rs:332-359` | Added exact-match tests for Morning (8:30), Afternoon (15:30), and Dusk (18:00) — now all 7 keyframes tested. |
| TD-004 | Weak Tests | P2 | `src/lib.rs:412-419` | Replaced no-assertion `let _p` with real assertions: bg non-black and fg-bg contrast >= floor. |
| TD-005 | Weak Tests | P2 | `src/lib.rs:296` | Added `test_compute_palette_with_non_default_config` — verifies strict/lax configs produce expected contrast changes. |
| TD-006 | Weak Tests | P3 | `src/lib.rs:362-372` | Added all 5 missing interpolation midpoint tests (Morning→Midday, Midday→Afternoon, Afternoon→Dusk, Dusk→Night, Night→Midnight). |
| TD-007 | Weak Tests | P3 | `src/lib.rs:395-403` | Strengthened `test_every_hour_produces_valid_palette` to assert all 7 color slots are non-black and fg != bg. |

## Progress Log

- 2026-05-07: All 7 TODO items resolved. 18→27 tests (9 added). Discovery scan found no new debt.
