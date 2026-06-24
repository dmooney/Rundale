# visual-atom-auditor-m12

M11 exposed a useful failure mode: raster compositing can reveal hard
rectangular PNG crop edges. A screenshot can catch it, but a local asset audit
should catch the easy cases before a browser proof.

## Approach

- Keep the auditor in `parish/apps/visual/scripts/` because it validates the
  visual-client asset contract.
- Parse PNG dimensions and alpha edges directly from bytes using Node's
  standard library. No new dependency is needed.
- Focus the first pass on The Crossroads because it is the current proof scene
  for reusable sprite compositing.
- Make failures practical and specific: include the layer id, asset id, image
  path, and measured values.

## Future Extensions

- Run all scenes once Kilteevan/Pub have equally reusable kits.
- Emit JSON so a generated-asset pipeline can consume the audit.
- Add screenshot-aware checks for repeated-stamp patterns and scale drift.
