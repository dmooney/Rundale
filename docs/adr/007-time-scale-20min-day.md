# ADR-007: Time Scale: 20 Minutes per Day

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Superseded (2026-03-23) — Default changed to 40 minutes per day (speed factor 36.0). The original 20-minute pace felt too fast in practice. Runtime-adjustable speed presets now let players choose: Slow (80 min/day), Normal (40 min/day), Fast (20 min/day, the original), or Fastest (10 min/day). See [Time System](../design/time-system.md) for details.

## Context

Rundale simulates a living Irish parish where seasons change, relationships evolve, and years pass. The time scale must balance several competing concerns:

- **Seasonal change must be visible**: Players should experience all four seasons (marked by the Irish festivals Imbolc, Bealtaine, Lughnasa, and Samhain) within a reasonable play session.
- **Relationships must evolve**: NPCs need enough simulated time for their relationships, routines, and lives to develop noticeably.
- **Inference budget**: Each game-tick that involves NPC cognition costs GPU time. Faster time scales mean more ticks per real minute, increasing inference demand.
- **Playability**: A single play session (2-3 hours) should feel meaningful and show the passage of time.
- **NPC schedules**: Daily routines (wake, work, eat, socialize, sleep) must fit within the compressed day cycle.

## Decision

Adopt the following time scale:

| Real Time | Game Time |
|-----------|-----------|
| 20 minutes | 1 day |
| ~7-8 minutes | Night portion |
| ~30-45 minutes | 1 season |
| 2-3 hours | 1 year |

This gives approximately **6-9 in-game days per season**.

The four Irish seasonal festivals mark the transitions:

- **Imbolc** (start of spring) -- ~February 1
- **Bealtaine** (start of summer) -- ~May 1
- **Lughnasa** (start of autumn) -- ~August 1
- **Samhain** (start of winter) -- ~November 1

These festival dates serve as temporal hooks for future mythological content.

The pacing matches Minecraft's time scale (20 real minutes per game day), which has proven comfortable for players in that context.

## Consequences

**Positive:**

- A full year is experienced in a single 2-3 hour play session
- Seasonal changes are visible and meaningful: weather shifts, NPC behavior adapts, festivals occur
- Relationships evolve noticeably over a session: friendships form, rivalries develop, gossip spreads
- The Minecraft-equivalent pacing is proven to feel natural to players
- Night portion (~7-8 real minutes) is long enough to create atmosphere but short enough to not bore players
- Multiple years of play show parish evolution across sessions

**Negative:**

- Tight inference budget per game-tick: at 20 minutes per day, each "hour" of game time is ~50 real seconds, limiting how many NPC cognition cycles can run per game-hour
- NPC daily schedules must be heavily compressed: a full day of wake-work-eat-socialize-sleep in 20 real minutes
- Some unrealism is unavoidable: conversations that should take 30 game-minutes happen in ~25 real seconds
- Movement between locations consumes significant portions of the game day (a 10 game-minute walk is ~8 real seconds)
- Balancing "enough happens" with "not too fast to follow" requires careful tuning

## Alternatives Considered

- **Real-time 1:1 mapping**: One real minute equals one game minute. No seasonal change would ever be visible. A single game day would take 24 real hours. Completely impractical for experiencing the passage of time.
- **Slower scale (1 real hour = 1 game day)**: More breathing room per game-tick but a full year would take 15+ real hours across multiple sessions. Seasonal change would be too slow to notice within a single session.
- **Player-controlled time**: Let the player advance time manually ("wait until evening"). Breaks immersion and the living-world illusion. NPCs would feel like they only exist when the player fast-forwards.
- **Adaptive time scale**: Speed up time when nothing is happening, slow down during interactions. Complex to implement and disorienting for the player. Makes it hard to maintain a consistent world simulation rate.

## Related

- [docs/design/time-system.md](../design/time-system.md)
- [ADR-002: Cognitive Level-of-Detail Tiers](002-cognitive-lod-tiers.md)
