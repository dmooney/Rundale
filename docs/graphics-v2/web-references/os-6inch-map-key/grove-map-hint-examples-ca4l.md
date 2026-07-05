# Grove Map Hint Examples CA4L

These examples support the clean OS 6-inch key reference for map-reader
annotation. The labels are general symbol classes, not location-specific
instructions.

Use:

- `os-6inch-map-key-reference-sheet.png` as the clean map key,
- `grove-map-hint-examples-ca4l.png` for reviewable examples,
- `os-6inch-map-key-reference-sheet-grove-examples-ca4l.png` when a single
  combined reference image is needed for a model.

CA4L supersedes CA4K by adding explicit positive/negative contrast cards:

- `Unfenced path / track`: positive examples show paired light/broken route
  edges; the negative example is a single bold dotted chain.
- `Rough vegetation / ditch block`: positive examples show OS rough-vegetation
  symbols and a boundary/ditch strip; the negative example is regular internal
  planting texture within a planted enclosure.

| #   | Hint                                    | Example source                                                                                              |
| --- | --------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| 1   | Deciduous trees                         | Grove crop `(690, 145, 850, 365)`                                                                           |
| 2   | Coniferous trees                        | Grove crop `(0, 455, 260, 525)`                                                                             |
| 3   | Planted / mixed enclosure               | Grove crop `(280, 155, 560, 405)`                                                                           |
| 4   | Unfenced path / track                   | OS key `(1260, 650, 1585, 720)`, Grove crop `(0, 405, 465, 525)`, negative Grove crop `(0, 545, 405, 630)`  |
| 5   | Dotted administrative / survey boundary | Grove crop `(0, 545, 405, 630)` and `(300, 525, 380, 820)`                                                  |
| 6   | Road plus administrative boundary       | Grove crop `(0, 0, 310, 185)`                                                                               |
| 7   | Single solid boundaries                 | Grove crop `(890, 600, 1198, 815)`                                                                          |
| 8   | Double solid corridor                   | Grove crop `(705, 0, 895, 245)`                                                                             |
| 9   | Rough vegetation / ditch block          | OS key `(65, 470, 805, 735)`, Grove crop `(580, 660, 900, 820)`, negative Grove crop `(280, 155, 560, 405)` |
| 10  | Roofed structures                       | Grove crop `(485, 285, 675, 455)`                                                                           |
| 11  | Map text                                | Grove crop `(410, 465, 640, 585)`                                                                           |
