# parish-palette — Technical Debt

## Open

*(none — discovery scan complete, no new debt found)*

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
