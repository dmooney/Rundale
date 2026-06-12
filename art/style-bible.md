# Rundale Diorama — Art Style Bible

## Setting

1820 rural Irish parish (County Roscommon). Every image must feel like a
specific, remembered place — not a generic fantasy village, not a cleaned-up
tourism poster.

## Visual style

- **Medium:** 16-bit pixel art, top-down 3/4 perspective view
- **Native plate dimensions:** 480x270 pixels (displayed with CSS
  `image-rendering: pixelated`)
- **Native sprite dimensions:** 48x72 pixels, transparent background
- **Palette:** muted earth tones — peat dark, straw yellow, whitewash,
  moss green, slate grey, turf brown, pale sky. No saturated primary colours.
- **Mood:** inviting but slightly melancholy. A specific Irish place with
  memory, poverty, weather, gossip, and social pressure. Not cozy, not
  cartoonish.

## Required visual elements (exterior plates)

- Muddy paths and puddles
- Low dry stone walls, occasional hedgerows
- Small irregular fields and strips of bog
- Thatched or slate-roofed cottages with whitewashed walls
- Hand-made, asymmetric, weather-worn everything

## Required visual elements (interior plates)

- Low ceiling, exposed rafters or thatch from inside
- Turf fire or open hearth (when appropriate)
- Rush-light or tallow candle lighting
- Simple handmade furniture — stools, a settle, a rough table
- Smoky atmosphere

## Negative rules (hard constraints — never include)

- No UI text, labels, signs, or readable words baked into the image
- No player character, avatar, or protagonist figure
- No fantasy elements: no magic circles, no elves, no anachronistic technology
- No clean "cottagecore" aesthetic: this is poverty and mud, not Pinterest
- No baked NPC characters in background plates (characters are placed
  dynamically by the engine)
- No modern colours or materials
- No speech bubbles, health bars, or game UI of any kind
- No dramatic lighting that removes the mundane quality of the scene

## Consistency requirements

- Every plate and sprite in the same session must feel like they were drawn
  by the same artist for the same game
- When anchor assets exist, pass them as reference images to the provider
  to enforce style consistency across generations
- Anchor assets are the first accepted plate and the first accepted sprite;
  they are marked `anchor: true` in the manifest

## Generation parameters

- **Plates:** generate at 1536x1024, downscale to 480x270 with Lanczos
- **Sprites:** generate at 1024x1024 with transparent background, downscale
  to 48x72 with Lanczos, trim transparent border first
