# Notebook Person-Art References

`accepted-roisin-chat-portrait-source.png` is the issue #1628 Roisin portrait
sample the user explicitly accepted. It was generated for this issue from the
Illustrated Parish Notebook direction; it is not prior experimental game art
and is not shipped at runtime.

The source image contains a baked checkerboard. The historical keyed derivative
`accepted-roisin-chat-portrait-style-keyed.png` deterministically replaces the
light checkerboard with the pipeline's flat `#ff00ff` key while retaining the
sparse dark linework.

`accepted-roisin-chat-marker-source.png` is the corresponding issue-approved
Roisin painted-world marker sample. The 1024x1024 historical
`accepted-roisin-chat-marker-style-keyed.png` derivative isolates that figure,
scales it to 45% canvas height, and centers it on the production key. It teaches
the earlier marker scale and line/wash target.

## Production Reference Boundary

The two full-character Roisin derivatives are retained as calibration and
review history, but **are not uploaded in production generation**. A controlled
schema-v2 ablation showed that image-edit conditioning copied Roisin's face,
hair, shawl, apron, and pose into unrelated women even when their structured
geometry differed and the prompt said “style only.”

`generation-config-v1.json` now uploads only
`docs/graphics-v2/illustrated-parish-notebook.png`, the authoritative concept
named in the issue. The model reads the notebook portrait surface from the UI
and the marker surface from the painted world while identity comes from each
NPC's structured facial geometry. The Roisin files remain useful for human
comparison, not as shared provider identity priors.

The keyed derivative was produced with:

```sh
magick -size 1254x1254 xc:'#ff00ff' \
  \( -size 1254x1254 xc:'#36362e' \
     \( accepted-roisin-chat-portrait-source.png \
        -colorspace gray -negate -level 7%,78% \) \
     -alpha off -compose CopyOpacity -composite \) \
  -compose Over -composite -resize 1024x1024 \
  accepted-roisin-chat-portrait-style-keyed.png
```

The keyed marker derivative was produced with:

```sh
magick -size 1024x1024 xc:'#ff00ff' \
  \( accepted-roisin-chat-marker-source.png \
     -fuzz 20% -transparent '#ff00ff' -trim +repage -resize x461 \) \
  -gravity center -compose Over -composite \
  accepted-roisin-chat-marker-style-keyed.png
```

The full Illustrated Parish Notebook concept is both the source art direction
and the sole production provider reference. No unrelated experiment, prior art
cycle, or named full-face derivative is part of the generation request.
