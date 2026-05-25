# Repository Layout

```
parish/
  crates/              14 workspace members (types, config, world, npc, etc.)
  apps/ui/             Svelte 5 + TypeScript frontend
  testing/fixtures/    scripted gameplay fixtures
  scripts/             Maintenance and quality gate scripts
mods/rundale/          Rundale game content (world, NPCs, prompts, lore)
deploy/                Dockerfile
docs/                  design, ADRs, plans, research, agent guides
justfile               Top-level proxies for common tasks
```

For the full crate-by-crate breakdown and module ownership rules, see
[docs/agent/architecture.md](agent/architecture.md) and
[docs/agent/codebase-map.md](agent/codebase-map.md).
