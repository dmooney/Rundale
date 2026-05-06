# parish-palette — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Stale Docs | P2 | `src/lib.rs:30` | Doc comment references `crate::gui::theme::GuiPalette` — no such type exists in this crate or anywhere in the workspace. The mirror type is `parish_core::ipc::types::ThemePalette`. |
| TD-002 | Stale Docs | P2 | `src/lib.rs:58` | Doc comment references `TimeOfDay` — no such type exists in this crate. The crate operates directly on anchor-hour floats without a `TimeOfDay` enum. |
| TD-003 | Weak Tests | P2 | `src/lib.rs:332-359` | Only 4 of 7 keyframes have exact-match tests. Missing: Morning (8:30), Afternoon (15:30), and Dusk (18:00). |
| TD-004 | Weak Tests | P2 | `src/lib.rs:412-419` | `test_compute_palette_all_hours_valid` contains zero assertions — binds `_p` and returns. Test name implies validation but only verifies the call doesn't panic. |
| TD-005 | Weak Tests | P2 | `src/lib.rs:296` | `compute_palette_with_config` has no tests with non-default `PaletteConfig` values. The contrast floor values (80.0 fg-bg, 45.0 muted-bg) are exercised only through the `Default` impl. |
| TD-006 | Weak Tests | P3 | `src/lib.rs:362-372` | Only one of six keyframe-to-keyframe interpolation midpoints is tested (Dawn→Morning). Missing: Morning→Midday, Midday→Afternoon, Afternoon→Dusk, Dusk→Night, Night→Midnight. |
| TD-007 | Weak Tests | P3 | `src/lib.rs:395-403` | `test_every_hour_produces_valid_palette` only asserts `bg != (0,0,0)` — trivial check that passes for all actual keyframes. Does not validate channel ranges, contrast, or color plausibility. |

## In Progress

*(none)*

## Done

*(none)*
