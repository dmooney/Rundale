# Grove Map Hint Reference Pack CA4N

CA4N changes the packaging rather than the symbol classes:

- use `os-6inch-map-key-reference-sheet.png` as the clean OS key image,
- use `grove-map-hint-examples-ca4m.png` as the large examples/contrast sheet,
- use the source map crop separately as the annotation target.

Reason: the combined CA4M reference image made the hard YES/NO contrast
examples small. The CA4M clean-context test correctly separated the faint
path/track candidate from dotted administrative/survey linework, but it still
misclassified regular planted-enclosure texture as rough vegetation / ditch
block. CA4N tests whether giving the examples sheet at full size fixes that
without adding location-specific instructions.
