# Grove Map Hint Examples CA4M

These examples support the clean OS 6-inch key reference for map-reader
annotation. The labels are general symbol classes, not location-specific
instructions.

Use:

- `os-6inch-map-key-reference-sheet.png` as the clean map key,
- `grove-map-hint-examples-ca4m.png` for reviewable examples,
- `os-6inch-map-key-reference-sheet-grove-examples-ca4m.png` when a single
  combined reference image is needed for a model.

CA4M supersedes CA4L by expanding the two repeated failure modes:

- `Unfenced path / track candidate`: positive examples show paired light or
  broken route edges; negative examples show a single bold dot chain and an
  isolated weak single boundary.
- `Rough vegetation / ditch block`: positive examples show OS rough-vegetation
  symbols and a southern boundary/ditch strip; negative examples show ordinary
  yard clutter and regular planted-enclosure texture.

| # | Hint | Example source |
|---|---|---|
| 1 | Deciduous trees | Grove crop `(690, 145, 850, 365)` |
| 2 | Coniferous trees | Grove crop `(0, 455, 260, 525)` |
| 3 | Planted / mixed enclosure | Grove crop `(280, 155, 560, 405)` |
| 4 | Unfenced path / track candidate | OS key `(1260, 650, 1585, 720)`, Grove crop `(0, 405, 475, 525)`, negative Grove crops `(0, 540, 430, 625)` and `(690, 330, 785, 650)` |
| 5 | Dotted administrative / survey boundary | Grove crop `(0, 545, 405, 630)` and `(300, 525, 380, 820)` |
| 6 | Road plus administrative boundary | Grove crop `(0, 0, 310, 185)` |
| 7 | Single solid boundaries | Grove crop `(890, 600, 1198, 815)` |
| 8 | Double solid corridor | Grove crop `(705, 0, 895, 245)` |
| 9 | Rough vegetation / ditch block | OS key `(65, 470, 805, 735)`, Grove crop `(570, 655, 930, 820)`, negative Grove crops `(260, 390, 585, 535)` and `(280, 155, 560, 405)` |
| 10 | Roofed structures | Grove crop `(485, 285, 675, 455)` |
| 11 | Map text | Grove crop `(410, 465, 640, 585)` |
