# Notebook Person-Art References

`accepted-roisin-chat-portrait-source.png` is the issue #1628 portrait sample
the user explicitly accepted as the target drawing language. It was generated
for this issue from the Illustrated Parish Notebook direction; it is not prior
experimental game art and is not shipped at runtime.

The source image contains a baked checkerboard. The portrait-only API reference
`accepted-roisin-chat-portrait-style-keyed.png` deterministically replaces the
light checkerboard with the pipeline's flat `#ff00ff` key while retaining the
sparse dark linework. It is a style-transfer reference only. NPC identity,
clothing, expression, and props continue to come from each NPC's metadata.

`accepted-roisin-chat-marker-source.png` is the corresponding issue-approved
painted-world marker sample. The 1024x1024
`accepted-roisin-chat-marker-style-keyed.png` derivative isolates that figure,
scales it to 45% canvas height, and centers it on the production key. It teaches
marker scale, line/wash balance, palette restraint, complete feet, and the lack
of a ground plane; it is not an identity source.

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

The full Illustrated Parish Notebook concept remains the source art direction,
but its large painted scene is not sent directly in per-character API calls.
The two issue-produced, user-approved derivatives isolate the portrait and
marker languages without introducing unrelated experimental artwork.
