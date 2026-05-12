# parish-world

World graph, movement, weather, and environment state for Parish.

## Purpose

`parish-world` owns location topology and simulation-facing world state used by
NPC systems, UI snapshots, and persistence.

## Key modules

- `graph` and `geo` — location graph structure and geographic helpers.
- `movement` and `transport` — travel rules and path interaction.
- `weather` — weather transitions tied to game time.
- `description`, `encounter`, `session`, `wayfarers`, `weather_travel` — text and presentation helpers.

## Primary type

- `WorldState` — central container for clock, player location, graph, weather,
  logs, visited locations, and shared event/gossip/conversation state.
