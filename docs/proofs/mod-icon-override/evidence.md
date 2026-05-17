Evidence type: screenshot

The proof image `rundale-icon-64.png` is a 64px downsample from the mod-owned
Rundale app icon set. It exercises the small-size readability target for the
runtime icon override path.

Checks run:

- `cargo test -p parish-core game_mod::tests::test_mod_icon_paths`
- `npm run test -- app-icon.test.ts`
