# Cycle CB: Direct Overhead Map Style Samples

Experiment: transform the highest-resolution local historic map crop directly
into an overhead ink-and-watercolor game map tile. No sample artwork, no
isometric/2.5D stage, no crop-to-playable-camera step.

## Inputs

- Source crop: `../../map-sources/beechwood-map-crop-control-02.png`
  - Beechwood high-resolution map crop, `1956 x 1450`.
  - Treated as the only layout authority.
- Legend reference: `../../web-references/os-6inch-map-key/os-6inch-map-key-reference-sheet.png`
  - Used only in the `with-legend` runs as symbol interpretation help.
  - Not intended as style, layout, page design, typography, or visible content.

## Shared Prompt Constraints

All runs asked for:

- direct overhead, north-up, flat orthographic map rendering;
- preserve the full source extent as much as possible;
- no isometric, no perspective, no 2.5D, no low-camera scene;
- no sample artwork or prior generated plate as style reference;
- interpret printed map symbols into playable terrain;
- remove printed labels, large letters, parcel numbers, and typography as
  in-world objects;
- no UI, compass, border, visible text, people, animals, carts, smoke, modern
  features, photorealism, or fantasy-map decoration.

## Prompt Variants

| Variant              | Main prompt difference                                                                                                  | Output                                                   |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| A no legend          | Faithful transformation; delicate black-brown pen, transparent watercolor, handmade survey sketch feel.                 | `idea-cb-a-no-legend-faithful-ink-watercolor.png`        |
| B no legend          | Clearer playable game-map read; pale compacted earth roads, simple roof footprints, crisp navigation corridors.         | `idea-cb-b-no-legend-clear-game-map.png`                 |
| C no legend          | Looser painterly watercolor; pigment blooms, dry-brush ink, organic terrain mood, less literal.                         | `idea-cb-c-no-legend-loose-watercolor.png`               |
| D no legend          | Restrained antique field-atlas; cartographic clarity, thin boundaries, conservative interpretation.                     | `idea-cb-d-no-legend-field-atlas.png`                    |
| A with legend        | Same as A, but with OS key as symbol interpretation aid.                                                                | `idea-cb-a-with-legend-faithful-ink-watercolor.png`      |
| B with legend leak   | Same as B, with OS key aid; leaked a visible legend/key card into the render. Retained as negative evidence.            | `idea-cb-b-with-legend-clear-game-map-legend-leak.png`   |
| B2 with legend retry | Same as B with legend, but with stronger negative language against visible legends, key cards, panels, text, or insets. | `idea-cb-b2-with-legend-clear-game-map-no-key-retry.png` |
| C with legend        | Same as C, with OS key aid and strong no-visible-legend wording.                                                        | `idea-cb-c-with-legend-loose-watercolor.png`             |
| D with legend        | Same as D, with OS key aid and strong no-visible-legend wording.                                                        | `idea-cb-d-with-legend-field-atlas.png`                  |

## Quick Read

- Best visual/readability direction: `B2 with legend retry`.
- Most source-conservative accuracy candidate: `D no legend`.
- Useful negative result: `B with legend leak`; giving the model a visible key
  can improve interpretation but can also leak the key as an inset artifact.

See `idea-cb-overhead-map-style-contact-sheet.png` for the side-by-side matrix.
