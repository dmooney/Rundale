# Experiment A: Map Only Baseline

Result: PASS for the saved final asset.

- Built-in image generation was used once with the exact requested prompt.
- Final saved plate: `idea-a-map-only.png`, `1536x864`, 16:9.
- Visual spot-check: no UI, labels, signs, map pins, or visible text; roads/yards/gates remain readable; base environment only.
- Processing note: the raw built-in output was `1536x1024`; it was cropped and scaled to 16:9. A bottom-edge stream-like artifact in the raw candidate was excluded by the final crop.
