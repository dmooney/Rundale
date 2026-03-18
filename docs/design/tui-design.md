# TUI Design

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md)

## Layout

- **Top bar**: Location name, time of day (as a word, not a number — "late afternoon"), weather description, season, optional unicode weather/moon symbol
- **Main panel**: Text output — descriptions, dialogue, narration. This is where the game lives.
- **Bottom**: Input prompt. Subtle status line if core stats are needed later.

## Color System (24-bit True Color)

The TUI uses background and accent color gradients to represent time of day and weather. The player should feel time passing without being told explicitly.

### Time-of-Day Palettes

| Time of Day   | Palette                     |
|---------------|-----------------------------|
| **Dawn**      | Pale wash, soft yellows     |
| **Morning**   | Warming golds               |
| **Midday**    | Warm, bright tones          |
| **Afternoon** | Deepening golds             |
| **Dusk**      | Deep blues, amber           |
| **Night**     | Near-black, cold grey       |
| **Midnight**  | Darkest palette             |

### Weather Palette Modifiers

Weather modifies the base time-of-day palette:

| Weather   | Modifier                    |
|-----------|-----------------------------|
| Overcast  | Muted/desaturated           |
| Rain      | Cooler tones, grey cast     |
| Fog       | Heavily desaturated         |
| Clear     | Full saturation             |

### Transitions

Color transitions should be **gradual**, not stepped. The palette shifts smoothly as time passes and weather changes.

## Debug Panel

> Requires `--features debug`. See [Debug System](debug-system.md) for full details.

When the debug panel is active (toggled via `/debug panel` or `F12`), the layout splits to accommodate a live-updating dashboard:

```
┌─────────────────────────────────┬──────────────────────┐
│ Top bar: location | time | wx   │                      │
├─────────────────────────────────┤   Debug Panel        │
│                                 │                      │
│   Main text panel               │   [Overview] [NPCs]  │
│   (game output)                 │   [Inference] [Tasks] │
│                                 │                      │
│                                 │   (tab content)      │
│                                 │                      │
├─────────────────────────────────┤                      │
│ > player input                  │                      │
└─────────────────────────────────┴──────────────────────┘
```

- The debug panel takes ~35% of terminal width
- On narrow terminals (< 120 columns), renders as a bottom split instead
- Navigate tabs with `Tab`/`Shift+Tab`, scroll with arrow keys
- Color-coded health indicators: green (healthy), yellow (warning), red (error)
- The panel is a separate ratatui widget; it does not affect game layout when hidden

## Terminal Compatibility

Target terminals (all support 24-bit RGB):

- kitty
- alacritty
- wezterm
- Windows Terminal

## Related

- [Time System](time-system.md) — Day/night cycle drives palette selection
- [Weather System](weather-system.md) — Weather modifies color palettes

## Source Modules

- [`src/tui/`](../../src/tui/) — Ratatui terminal UI, color system, layout
