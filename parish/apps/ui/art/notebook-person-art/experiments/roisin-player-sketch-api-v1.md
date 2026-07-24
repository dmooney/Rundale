# Roisin Player-Notebook API Calibration V1

Goal: make the production OpenAI API pipeline reproduce the issue-approved
portrait language: a sparse, uncolored pen-and-ink observation plausibly drawn
by the player character in their working notebook.

## Root Cause

The first API request flattened portrait and marker direction into one long
prompt, including painted-world watercolor and color language, and attached the
full concept image whose painted scene dominates its tiny UI portrait. The
validator checked dimensions, key coverage, and nonblank content but not scale,
fill, or line density. The result was a polished, densely modeled illustration
that was period-plausible but wrong for the UI fiction.

## Corrective Contract

- Portrait prompts now lead with the diegetic player-sketch lore.
- Portrait and marker prompts no longer share flattened medium/palette text.
- Portrait requests receive the accepted sparse chat portrait as a style-only
  reference; marker requests retain the full Illustrated Parish Notebook
  concept reference.
- The keyed raw contract requires `#ff00ff` to remain visible through uninked
  areas of the face, hair, clothing, and prop.
- Portrait validation now gates drawing height, total subject fill, dark-ink
  coverage, density inside the ink bounds, light fill, and colored fill.
- Key removal normalizes retained portrait strokes to graphite `#36362e` while
  preserving the provider-generated line geometry and alpha.

## Result

Provider/model: OpenAI `gpt-image-2-2026-04-21`, `/v1/images/edits`, high
quality, 1024x1024 opaque PNG raw response.

- Provider request ID: `req_7714675a70ee4189a224628f7bf37e5f`
- Usage: 1,773 input tokens; 7,024 output image tokens; 8,797 total
- Raw SHA-256: `017670fa7c12a9a39c9d4faa3cd312aa6e397cf04447e2bb16678664bba5f299`
- Candidate SHA-256: `9a52c020f3a8697c0fca85a2d64cc91ac420cddc0b8c9c87f4e8262f01c0d30f`
- Job ID: `29e29f2de7050009672abe7b0db28b8075c89e4563c3d3e43bfbd6f026a6a6e9`

The agreed chat target has approximately 4.82% dark-line coverage and a 61.96%
line-bound height. The revised API candidate has 4.26% visible coverage and a
60.74% ink-bound height. The earlier wrong API result used 40.82% subject
coverage and 92.48% subject-bound height.

Visual comparison:

- `roisin-chat-vs-api-player-sketch-v1.png`
- `roisin-api-player-sketch-v1-parchment-preview.png`

Candidate receipt:

- `../candidates/objects/29/29e29f2de7050009672abe7b0db28b8075c89e4563c3d3e43bfbd6f026a6a6e9/receipt.json`

Review packet:

- `../candidates/review-packets/roisin-player-sketch-v1/review.html`

The candidate passes automated production checks and remains `pending` human
review. It is not approved or eligible for runtime promotion until the review
decision is explicitly recorded.
