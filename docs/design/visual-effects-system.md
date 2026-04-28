# Visual Effects System — Design

Rundale is a text-based game, and that's a feature, not a limitation. But text and CSS give us
a surprising canvas for effects that feel alive: atmospheric overlays, typographic trembling,
creatures that cross the screen, glows that breathe from behind the text. The goal is to
reward players who pay attention — a storm that announces itself before the narrative does,
a fairy that lands uninvited on a sentence, a bog light that appears at the edge of night.

All effects are implemented in CSS and Svelte. No WebGL, no canvas, no video files. The
system is layered on top of the existing theme/palette pipeline.

---

## Design Principles

1. **Diegetic first.** Effects mirror what's true in the game world. If it's raining in
   Kilteevan, rain falls on the UI. Players shouldn't need to "turn off" effects — they're
   news, not decoration.

2. **Text is sacred.** Nothing permanently obscures dialogue or system messages. Overlay
   effects use `pointer-events: none` and respect readability. Transient sprites
   arrive and depart; they don't linger forever.

3. **Infrequent enough to surprise.** Most effects have cooldown windows. A fairy
   doesn't visit twice in five minutes. Lightning doesn't flash on every overcast word.
   Rarity makes them feel like actual events.

4. **CSS-native where possible.** Animations use `@keyframes`, `clip-path`, `filter`, `mix-blend-mode`,
   and `text-shadow` — no external dependencies, no canvas, no JS animation loops beyond
   what's already running.

5. **Triggered by real game state.** Every effect has a defined trigger condition tied to
   data already present in `WorldSnapshot`, the text log, or the event bus.

---

## Architecture Overview

### Effect Manager (Svelte store + component)

A new `effects.ts` store tracks active and queued effects. It exposes:

```ts
// What's currently on screen
interface ActiveEffect {
  id: string;
  type: EffectType;
  startedAt: number;       // real ms timestamp
  durationMs: number;
  payload?: Record<string, unknown>;
}

// The store
const activeEffects: Writable<ActiveEffect[]>;

// Called by trigger evaluators
function spawnEffect(type: EffectType, durationMs: number, payload?): void;
function clearEffect(id: string): void;
```

A top-level `EffectsLayer.svelte` component sits as a sibling to `ChatPanel` in the app
shell, absolutely positioned to fill the viewport, `pointer-events: none`. It renders
whichever effects are currently active.

### Trigger Evaluators

Triggers are pure functions evaluated whenever `worldState` or `textLog` changes:

```ts
type Trigger = (
  world: WorldSnapshot,
  textLog: TextLogEntry[],
  activeEffects: ActiveEffect[],
  lastEvaluated: number
) => SpawnRequest | null;
```

Triggers are registered in a central `TRIGGERS` array. Each evaluator handles its own
cooldown logic by consulting `lastEvaluated` and the currently active effect list.

### Cooldown Registry

A separate `cooldowns.ts` map keyed by `EffectType` stores the last spawn timestamp.
Triggers check this before firing.

---

## Effect Catalogue

### 1. Lightning Flash

**Trigger:** Weather is `Storm` or `HeavyRain`, time is evening or night. Random chance
(~15% per minute of storm weather, evaluated on `worldState` updates).

**What it does:**
A fast, two-stage flash:
1. The entire viewport bleeds to near-white (`background: rgba(255,255,240,0.92)`) over
   60ms, then snaps back in 200ms.
2. Simultaneously, all text briefly inverts (dark on light, the reverse of night palette).
3. Optionally, 80–300ms later (randomised), a second smaller flash — the afterimage.

**Implementation:**
A `<div class="lightning-overlay">` fades in/out with a short `@keyframes lightning-flash`
animation. CSS `mix-blend-mode: screen` over the chat panel for the text inversion effect.

**Cooldown:** 90 seconds minimum between flashes.

---

### 2. Rain on Glass

**Trigger:** Weather is `LightRain`, `HeavyRain`, or `Storm`.

**What it does:**
Vertical streaks descend across the chat panel — short, thin, semi-transparent lines with
slight `blur` and a `linear-gradient` body that fades out at the bottom. They drift at
slightly different speeds and horizontal positions, created with `nth-child` offset delays.
Heavy rain has more streaks, faster. Light rain is sparse and slow.

**Implementation:**
`<div class="rain-layer">` containing 20–40 `<span class="raindrop">` elements.
Each raindrop is a `2px × 18px` element with `animation: fall Xs linear infinite` and a
per-element `animation-delay` derived from its index. Streaks are `position: absolute`
and use `will-change: transform` for GPU compositing.

**Visual note:** Opacity is kept low (~0.18). These are ghost-rain, felt rather than seen.

---

### 3. Fog Creep

**Trigger:** Weather is `Fog`, or time is `Midnight`/`Dawn`.

**What it does:**
A low-opacity, soft white vignette breathes in from the bottom and sides of the chat
panel. The fog shifts slowly — a very long `@keyframes fog-drift` (30–60s cycle)
that nudges a radial gradient's center position by a few percent.

At `Fog` intensity the vignette extends higher, partially softening the lower half of
the chat log. At `Midnight` it's a faint edge-only haze.

**Implementation:**
A `::after` pseudo-element on `.effects-layer` with `background: radial-gradient(...)` at low
opacity. Blend mode `lighten` so it only lightens, never darkens over text.

---

### 4. Northern Lights / Aurora

**Trigger:** Clear night in winter or late autumn (hour 22–04, weather `Clear`,
season `Winter` or `Autumn`). Rare (5% chance per 3-minute window, max once per session).

**What it does:**
A faint aurora ripples across the top portion of the viewport — horizontal bands of
pale green, violet, and cyan that drift slowly left-to-right and pulse in brightness.
The effect is entirely behind the UI chrome, like a glow through a window.

**Implementation:**
`position: fixed; top: 0; left: 0; width: 100%; height: 40vh; z-index: 0`
with a `background: linear-gradient(180deg, ...)` containing three colour stops that
animate via `@keyframes aurora-shift`. The gradient hue shifts using CSS `hue-rotate`
filter on the whole element. Opacity is kept at 0.12–0.18 so it doesn't overwhelm the
dark night palette.

An entry message may optionally appear in the system log: *"A shimmer of pale light dances
above the horizon."*

---

### 5. Bog Light (Will-o'-the-Wisp)

**Trigger:** Player is at or near a bogland location at night or dusk.
(Location name contains "bog" or "moor", hour 19–04.)

**What it does:**
A soft, ghostly orb — warm yellow-green — drifts slowly across the background of the
chat panel in an irregular Lissajous-like path. It's small (40px diameter), blurry
(`filter: blur(12px)`), and very faint (opacity 0.15). It wanders for 2–4 minutes,
occasionally pausing, then fades out.

A second wisp may spawn 30–90 seconds later with a slightly different colour (cooler
green-blue) and different path timing.

**Implementation:**
`<div class="wisp">` with `border-radius: 50%; background: radial-gradient(...)` and
`position: absolute`. Movement via `@keyframes wisp-wander` using `translate()` that
visits 6–8 waypoints in a cubic-eased sequence. Each wisp element has unique keyframe
names generated at spawn time via inline `<style>` injection.

**Lore note:** In Irish tradition, will-o'-the-wisps (*tine sídhe*, fairy fire) lead
travellers astray. The wisps never stay still — they always seem just about to go
somewhere else.

---

### 6. The Fairy

**Trigger:** A special rare event. Probability rolls when:
- Player is near a fairy fort (ráth) location, OR
- It is Bealtaine (May 1) or Samhain (November 1), OR
- An NPC mentions the fairies (text log scan for "fairy", "faerie", "sídhe", "púca")
- Chance: ~3% per qualifying minute; cooldown 20 minutes minimum.

**What it does:**
The fairy enters from one of the four screen edges, flies in a curving arc toward a
specific word in the most-recent system message, "lands" on it (a soft glow appears
under the word), flutters briefly (the word shakes gently), then lifts off and exits
the other side. Total duration: 8–20 seconds.

**Sprite:**
Two frames of SVG: wings-up and wings-down (roughly 24×32px). CSS alternates between
them at 12fps using `animation: wingbeat 0.083s steps(1) infinite`. The sprite is
faintly luminous — `filter: drop-shadow(0 0 6px rgba(200,255,160,0.9))`.

**Flight path:**
A cubic Bézier curve from entry point to target word coordinates, computed in JS.
The sprite uses `motion-path: path(...)` (CSS `offset-path`) to follow the curve.
Or fallback: a JS RAF loop updating `transform: translate(x,y)`.

**Text effect on landing:**
The targeted word gains a CSS class `fairy-touched` that applies `text-shadow: 0 0 8px #c8ffa0`
and a gentle `@keyframes fairy-shimmer` (alternating opacity 0.8–1.0 over 0.6s).
On departure the shimmer fades over 1s and `fairy-touched` is removed.

**Word targeting:**
Scans the most recent text log entry's rendered `<span>` elements and picks a
semantically interesting word: nouns like "night", "road", "river", "God", "soul",
"field", "flame". Falls back to the last word if none match.

---

### 7. Wind-Shudder Text

**Trigger:** Weather is `Storm` or `HeavyRain`, random brief pulses every 40–90 seconds.

**What it does:**
A single recent system message (not NPC dialogue — those belong to their speaker)
trembles briefly as if a gust rattled the window. Individual words shift by 1–2px
in alternating horizontal directions with a 80ms period over 600ms total.

**Implementation:**
The target `<p>` element gets class `wind-shudder` which applies:
```css
@keyframes wind-shudder {
  0%   { transform: translateX(0); }
  15%  { transform: translateX(-2px) rotate(-0.2deg); }
  35%  { transform: translateX(1.5px) rotate(0.15deg); }
  55%  { transform: translateX(-1px); }
  75%  { transform: translateX(0.5px); }
  100% { transform: translateX(0); }
}
```
`animation: wind-shudder 0.6s ease-in-out 1` (plays once, then class is removed).

This is the subtlest effect in the system — players may not consciously notice it,
but it contributes to unease during storms.

---

### 8. Ember Drift

**Trigger:** Player is at a location with a fire (smithy, tavern hearth, festival bonfire
during Bealtaine/Samhain). Time is evening or night.

**What it does:**
Small orange-red particles rise slowly from the bottom of the chat panel, drifting
slightly left and right as they ascend, fading out by the time they reach the midpoint
of the screen. They appear in irregular bursts of 2–5, not a constant stream.

**Implementation:**
`<span class="ember">` elements, roughly 3px × 3px, `border-radius: 50%`,
`background: radial-gradient(circle, #ff9944, #ff5500)`.
`@keyframes ember-rise` uses `translate()` with a slight sinusoidal horizontal component
(`translateX(calc(sin(...) * 8px))` — approximated with keyframe offsets).
Opacity goes 0 → 1 → 0 over the animation duration (2.5–4s, varied per ember).

Embers spawn at random horizontal positions along the bottom 10% of the chat panel.

---

### 9. Frost Crystals

**Trigger:** Season is `Winter`, weather is `Clear` or `Overcast`, time is `Night`
or `Midnight` or `Dawn`.

**What it does:**
The borders and panel edges accumulate faint crystalline patterns — thin, branching
lines that slowly grow inward from the corners and top edges of the chat panel and
sidebar. They don't reach text, stopping at the padding boundary.

**Implementation:**
SVG `<path>` elements (pre-authored, 3–4 variants rotated/flipped) positioned at
corners with `stroke-dashoffset` animation to "draw" themselves in over 8–12 seconds.
Colour: `rgba(200, 230, 255, 0.25)` — icy blue-white, very faint.
On weather change away from frost conditions, they fade and retract (reverse animation).

---

### 10. Time-of-Day Shimmer

**Trigger:** Hour transitions (automatically, as the clock crosses 5:00, 7:00, 12:00,
17:00, 20:00, 23:00 — the palette keyframe boundaries).

**What it does:**
At each major time-of-day boundary, the palette transition (which already happens) is
accompanied by a brief, subtle radial pulse from the centre of the screen — a wash of
the incoming palette's dominant colour that expands outward and fades in 1.5 seconds.

**Implementation:**
A `<div class="dayshift-pulse">` with `border-radius: 50%; transform: scale(0)` transitions
to `scale(3)` while `opacity` goes from 0.3 to 0, using `mix-blend-mode: soft-light`.
The colour is derived from the new palette's `--color-accent`.

This is the most "always-on" ambient effect — subtle enough that most players won't consciously
notice it, but it makes the world feel like it breathes.

---

### 11. Festival Candle-glow

**Trigger:** A festival is active (`festival` field non-null in `WorldSnapshot`).

**What it does:**
Warm candle-light blobs pulse gently at the corners of the viewport — soft amber
radial gradients (think votive candles seen through frosted glass) that slowly breathe
(scale 1.0 → 1.08 → 1.0 over 3–4s). The effect is visible throughout the festival day.

For Samhain specifically, the colour shifts to a cooler orange-violet and the pulse is
slightly less regular — as if the candles are guttering.

---

### 12. Crows at Dusk

**Trigger:** Time transitions to `Dusk`, season is `Autumn` or `Winter`.

**What it does:**
A flock of 5–12 small black silhouettes (simple hand-drawn SVG glyphs — more calligraphic
stroke than sprite) arc across the top of the viewport from right to left over 6–10 seconds.
Each crow has a slightly different flight path offset and speed. They don't return.

**Implementation:**
SVG paths for each crow variant (3–4 variants). Elements are `position: fixed; top: 0–12vh`
and animated with `@keyframes crow-fly` using `translateX(-110vw)`. Each has a different
`animation-duration` (6.5s–9s) and `animation-delay` (0–2s stagger). The whole group
spawns once per dusk transition.

This is the most visually complex effect but remains pure CSS/SVG with no raster images.

---

### 13. Ink Bleed (NPC Emotional Intensity)

**Trigger:** NPC dialogue that includes strong emotional language — grief, fury, terror,
joy. Detected by keyword scan of the most recent NPC message.

**What it does:**
When an NPC utters something emotionally intense, their chat bubble briefly bleeds a faint
coloured wash from the bubble's background — a radial gradient in an emotion-mapped colour
(red for anger, blue for grief, gold for joy, grey for fear) that expands to the bubble
edges and fades over 1.5 seconds.

This is an impressionistic effect, not a HUD indicator. It should feel like the words
themselves are bleeding.

**Implementation:**
A `::before` pseudo-element on `.npc-bubble.ink-bleed` with `background: radial-gradient(...)`
and `animation: ink-expand 1.5s ease-out forwards`. Keywords mapped to CSS classes:
`ink-anger`, `ink-grief`, `ink-joy`, `ink-fear`.

---

## Trigger Data Flow

```
WorldSnapshot (weather, hour, season, festival, location_name)
    │
    ▼
TriggerEvaluator.ts
    │  evaluates on each worldState update and textLog update
    │  checks cooldowns, active effects, game state
    │
    ▼
effects store
    │  spawnEffect() / clearEffect()
    │
    ▼
EffectsLayer.svelte
    │  renders matching <Effect> components
    │  each component is self-managing (mounts, animates, unmounts)
    │
    └─→ DOM / CSS animations
```

Text-analysis triggers (Fairy, Ink Bleed) run on `textLog` updates, scanning the most
recent entry's content string.

---

## Priority and Conflict Rules

Some effects shouldn't coexist:

| If active      | Suppress          |
|----------------|-------------------|
| Lightning      | Ember Drift (while flash)  |
| Fog Creep      | Rain on Glass (opacity halved) |
| Aurora         | Bog Light (one atmospheric bg effect at a time) |
| Frost Crystals | Ember Drift       |
| Fairy          | Wind-Shudder (don't shake while fairy is visiting) |

The effect manager checks the `suppressedBy` field on each effect definition before
spawning. A suppressed effect is queued for up to 30 seconds before being discarded.

---

## Accessibility and Opt-Out

A `prefers-reduced-motion` media query wraps all `@keyframes` animations. When
`prefers-reduced-motion: reduce` is active:

- All animated effects are disabled.
- Static versions replace animated ones where meaningful (e.g., a static rain
  overlay instead of falling streaks; a static amber corner glow instead of pulsing candles).
- The Fairy simply doesn't visit.

A player-facing toggle ("Effects: On / Minimal / Off") will be surfaced in a future
settings panel. The store serialises this preference to `localStorage`.

---

## Implementation Phases

### Phase 1 — Infrastructure (no visible effects yet)
- `effects.ts` store with `ActiveEffect`, `spawnEffect`, `clearEffect`
- `cooldowns.ts` registry
- `EffectsLayer.svelte` component skeleton in app shell
- `TriggerEvaluator.ts` with one stub trigger (always-no-op)
- `prefers-reduced-motion` guard

### Phase 2 — Atmospheric overlays (weather + time)
- Rain on Glass
- Fog Creep
- Time-of-Day Shimmer
- Frost Crystals

### Phase 3 — Dramatic events
- Lightning Flash
- Wind-Shudder Text
- Ember Drift
- Crows at Dusk

### Phase 4 — Mythological / rare
- Bog Light (Will-o'-the-Wisp)
- Northern Lights / Aurora
- Festival Candle-glow
- The Fairy

### Phase 5 — NPC-reactive
- Ink Bleed

---

## Open Questions

- **Fairy word targeting:** Rendering coordinates of specific `<span>` elements require a
  `getBoundingClientRect()` call after the DOM settles. Need to handle scroll offset and
  panel resize gracefully.
- **Crow SVG paths:** Should be hand-crafted to look like brush strokes, not clip-art birds.
  Worth sourcing from someone with illustration taste.
- **Wisp movement:** Lissajous paths via pure CSS are possible but finicky. A small JS RAF
  function updating `transform` may be cleaner than abusing `@keyframes`.
- **Festival candles and Samhain mode:** How much do we lean into the horror register on
  Samhain? The original game lore (mythology-hooks.md) suggests the veil is thin — we could
  make the UI subtly wrong (colours slightly inverted, text slightly jittery) rather than
  just atmospheric candles.
- **Performance budget:** Rain and aurora together could stress lower-end hardware.
  Consider a hardware concurrency check (`navigator.hardwareConcurrency < 4`) to reduce
  particle counts automatically.
