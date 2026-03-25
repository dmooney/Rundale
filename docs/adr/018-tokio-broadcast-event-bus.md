# ADR 018: Use tokio::sync::broadcast for Event Bus

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Accepted — 2026-03-25

## Context

Phase 5 introduces multiple NPC tiers operating concurrently at different tick rates. Cross-tier communication is needed for:

- Weather changes affecting all NPCs regardless of tier
- Gossip events propagating between co-located NPCs
- Mood shifts and relationship changes crossing tier boundaries
- Tier transition signals when NPCs inflate/deflate

The event bus must support multiple producers (any tier can emit events) and multiple consumers (all tiers listen), with minimal coordination overhead.

## Decision

Use `tokio::sync::broadcast` with a channel capacity of 256 as the event bus.

Each event is a lightweight enum (`WorldEvent`) carrying the event type, source NPC/location, and payload. Producers call `sender.send(event)` and consumers hold `Receiver` handles obtained via `sender.subscribe()`.

## Consequences

**Positive:**
- Zero-copy broadcast to all subscribers with no explicit routing logic
- Naturally fits the tokio async ecosystem already used throughout Parish
- Backpressure via bounded capacity prevents unbounded memory growth
- Simple API: subscribe, send, recv — no custom pub/sub infrastructure needed

**Negative:**
- No persistence — events are ephemeral and lost on restart. Important state changes must be recorded separately (e.g., via the journal system)
- Bounded capacity (256) means slow consumers lose events via `RecvError::Lagged`. Tier 4's seasonal tick rate makes it the most likely to lag
- No replay — new subscribers cannot see events emitted before they subscribed

## Alternatives Considered

- **tokio::sync::mpsc per consumer**: Would require explicit fan-out logic and N channels instead of one broadcast
- **Custom ring buffer**: More control but significant implementation effort for marginal benefit
- **Persistent event log (SQLite)**: Too heavy for ephemeral simulation events; the journal system already handles durable events

## Related

- [Cognitive LOD](../design/cognitive-lod.md) — Tier system that the event bus connects
- [ADR 002: Cognitive LOD Tiers](002-cognitive-lod-tiers.md)
- Source: `crates/parish-core/src/world/events.rs`
