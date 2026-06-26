# Visual Generated Kilteevan Plate M9 Plan

## Implementation

1. Add a semantic Kilteevan plate spec under `mods/rundale/scene-recipes/` or a nearby visual-art directory. It should encode scene topology and art-direction constraints, not visible sprite placements.
2. Add a small generator/proof script that reads the spec and emits:
   - a deterministic image-generation prompt;
   - a prompt manifest with semantic sockets/hotspots;
   - validation summaries for road/water/bridge/prop constraints.
3. Generate one candidate full-scene PNG plate using the built-in image-generation path. Move the selected project-bound asset into `mods/rundale/assets/scenes/kilteevan-village/` with a non-destructive filename.
4. Integrate the selected plate into the Kilteevan visual proof/client path while keeping interactive hotspots, NPC slots, captions, and command fallback on top.
5. Render desktop and mobile screenshots, scrutinize them against the visual bar, and iterate the prompt or generated candidate until the image reads as the desired game direction.
6. Keep the old compositor/chunk proof useful as validation/tooling, but document that the generated plate is the visual target for Kilteevan.
7. Write evidence and judge, run gates, commit, push, and attach proof to PR #1605.

## Verification

```sh
npm --prefix parish/apps/visual run check
npm --prefix parish/apps/visual run test
npm --prefix parish/apps/visual run build
cargo test --manifest-path parish/Cargo.toml -p parish-mod scenes --all-targets
cargo test --manifest-path parish/Cargo.toml -p parish-core scene_state --all-targets
cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_visual-generated-kilteevan-plate-m9.txt
just agent-check
```

## Prompt Strategy

Use the user-provided reference image as the style target in prose: dense isometric pixel art, high 3/4 camera, readable village-game perspective, muted wet-earth palette, coherent scale, 1820s rural Ireland, integrated cottages and roads, and no modern objects.

The prompt should avoid baked UI text and labels for the first implementation. UI captions and location titles should be overlaid by the client where needed.

## Expected Commit Shape

- `feat: add generated Kilteevan plate spec`
- `feat: integrate generated Kilteevan plate proof`
- `test: cover generated plate prompt and scene proof`
- `docs: record generated plate visual direction`
