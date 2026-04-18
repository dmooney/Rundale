# parish-npc

NPC simulation, memory, schedules, and reaction systems.

## Purpose

`parish-npc` contains behavior and state models for non-player characters,
including tiered cognition and autonomous updates.

## Key modules

- `manager` — orchestration and tier management for NPC updates.
- `autonomous` / `ticks` — simulation loops and periodic updates.
- `memory` — short/long-term memory representations.
- `transitions` / `tier4` — cognition tier transitions and low-fidelity rules.
- `reactions`, `overhear`, `mood`, `anachronism` — dialogue/social subsystems.

## Notes

Shared schema types for NPC data files are re-exported for mod/editor tooling.
