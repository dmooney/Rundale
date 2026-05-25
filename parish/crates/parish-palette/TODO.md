# parish-palette — Technical Debt

## Open

*(none — 2026-05-25 discovery scan complete, no new debt found)*

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
| TD-008 | Dead/Unused Export | P2 | `src/lib.rs:296` | Reduced `compute_palette_with_config` visibility from `pub` to `pub(crate)` — no external consumers. |
| TD-009 | Duplication | P2 | `src/lib.rs:155-158,266,271-273` | Extracted `f32_to_u8_clamped` helper and replaced 4 repeated `.round().clamp(0.0, 255.0) as u8` sites. |
| TD-010 | Redundant Test | P2 | `src/lib.rs:523-527` | Deleted `test_compute_palette_produces_valid_colors` — fully covered by `test_compute_palette_all_hours_valid`. |
| TD-011 | Stale Docs | P2 | `README.md:14` | Updated pipeline diagram to include `compute_palette_with_config`. |
| TD-012 | Brittle Conditional | P2 | `src/lib.rs:224-225` | Replaced silent `KEYFRAMES[0]` fallback with `unreachable!` to fail fast on invariant violation. |

## Progress Log

- 2026-05-07: All 7 TODO items resolved. 18→27 tests (9 added). Discovery scan found no new debt.
- 2026-05-11: Resolved TD-008 through TD-012. 26→25 tests (1 removed). fmt/clippy/test clean.
- 2026-05-25: Refreshed scan against current source. Single-file crate remains small (626 LOC) with focused tests; no new TODO entries opened.
