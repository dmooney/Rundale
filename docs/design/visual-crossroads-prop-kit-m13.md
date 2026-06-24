# visual-crossroads-prop-kit-m13

M10 proved sprite reuse through subtle road-wetness atoms. M13 makes the
compositor more convincing by adding reusable physical prop families:
stone-wall pieces and bramble/foliage pieces. These are more visible than
puddles, so they must be subtle enough not to reveal repeated stamps.

## Scope

- Derive small transparent PNG atoms from the current Crossroads art.
- Add wall and bramble kit assets to `mods/rundale/scenes.json`.
- Place repeated instances with varied `x/y`, `z`, scale, flip, and opacity.
- Keep the broad wall/bramble local crops as continuity layers for now, but
  lower opacity where the kit starts carrying visible detail.
- Tighten the atom auditor so it expects multiple reusable kit families.

## Non-goals

- No new schema field is required.
- No SVG placeholders.
- No full procedural tile map yet.
- No claim that the wall/bramble kits are final production art.

## Risk

Wall and foliage repetition is easier to notice than puddle repetition. The
proof should favor modest opacity and slight variation, and screenshots should
remain the final judge.
