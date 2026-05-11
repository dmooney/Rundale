Evidence type: gameplay transcript

## Techdebt cleanup: parish-palette

### Summary

Resolved all 7 TODO items in `parish/crates/parish-palette/TODO.md`:

- **TD-001** (Stale Docs P2): Updated doc comment on `RawPalette` to reference `parish_core::ipc::types::ThemePalette` instead of nonexistent `crate::gui::theme::GuiPalette`.
- **TD-002** (Stale Docs P2): Updated doc comment on `KEYFRAMES` to remove stale `TimeOfDay` reference.
- **TD-003** (Weak Tests P2): Added exact-match keyframe tests for Morning (8:30), Afternoon (15:30), Dusk (18:00).
- **TD-004** (Weak Tests P2): Replaced no-assertion `test_compute_palette_all_hours_valid` with assertions checking bg non-black and fg-bg contrast >= floor.
- **TD-005** (Weak Tests P2): Added `test_compute_palette_with_non_default_config` exercising strict and lax PaletteConfig values.
- **TD-006** (Weak Tests P3): Added all 5 missing interpolation midpoint tests (Morning→Midday, Midday→Afternoon, Afternoon→Dusk, Dusk→Night, Night→Midnight).
- **TD-007** (Weak Tests P3): Strengthened `test_every_hour_produces_valid_palette` to assert all 7 color slots are non-black and fg != bg.

### Test output (27/27 pass)

```
running 27 tests
test tests::test_compute_palette_all_hours_valid ... ok
test tests::test_compute_palette_produces_valid_colors ... ok
test tests::test_compute_palette_with_non_default_config ... ok
test tests::test_ensure_color_contrast_adjusts_when_needed ... ok
test tests::test_ensure_color_contrast_noop_when_sufficient ... ok
test tests::test_contrast_floor_all_hours ... ok
test tests::test_contrast_floor_afternoon_dusk_transition ... ok
test tests::test_interpolation_midpoint_afternoon_dusk ... ok
test tests::test_every_hour_produces_valid_palette ... ok
test tests::test_interpolation_midpoint_dawn_morning ... ok
test tests::test_interpolation_midpoint_dusk_night ... ok
test tests::test_interpolation_midpoint_midday_afternoon ... ok
test tests::test_interpolation_midpoint_morning_midday ... ok
test tests::test_interpolation_midpoint_night_midnight ... ok
test tests::test_keyframe_afternoon_exact ... ok
test tests::test_keyframe_dawn_exact ... ok
test tests::test_keyframe_dusk_exact ... ok
test tests::test_keyframe_midday_exact ... ok
test tests::test_keyframe_midnight_exact ... ok
test tests::test_keyframe_morning_exact ... ok
test tests::test_keyframe_night_exact ... ok
test tests::test_lerp_color ... ok
test tests::test_lerp_u8_boundaries ... ok
test tests::test_luminance ... ok
test tests::test_midnight_wraparound_hour_0 ... ok
test tests::test_midnight_wraparound_hour_23 ... ok
test tests::test_smooth_transition_no_jumps ... ok

test result: ok. 27 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### Clippy output

```
cargo clippy -p parish-palette -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.06s
```

### Formatting

```
cargo fmt --check -p parish-palette
(no output - clean)
```

### Witness scan

```
just witness-scan
Scanning 2 changed file(s) for placeholder markers...
Witness scan passed.
```
