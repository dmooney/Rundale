# Agent Docs

Read the [current product specs](../product-specs/README.md) first for mobile reset
scope and milestone gates. This directory maps implementation and engineering
rules; read only the topics relevant to the task. Existing client/tool documents
remain scoped references, not a mandate to restore old product features.

| Task concern                                                   | Required reference                                           |
| -------------------------------------------------------------- | ------------------------------------------------------------ |
| Mobile architecture, Swift/Rust boundary, Endpoint integration | [mobile-architecture.md](mobile-architecture.md)             |
| Universal rules and routing to specialized invariants          | [engineering-rules.md](engineering-rules.md)                 |
| Existing runtime MCP/CLI commands                              | [runtime-driving-reference.md](runtime-driving-reference.md) |

Model selection and orchestration policy belongs in each agent's global
configuration and is not duplicated here. Claude Code and Codex both read
`AGENTS.md` natively, loading a subdirectory's `AGENTS.md` when they work in that
directory; put area-specific traps there rather than in the root file. Skills
live in `.agents/skills/` (`.claude/skills` is a symlink to it).

The detailed references below describe existing implementation and tooling.

For mobile acceptance cases, use the [Phase 1 and Phase 2 test plans](../test-plans/README.md)
alongside the full product milestone requirements.

| Topic                                                             | File                                                       |
| ----------------------------------------------------------------- | ---------------------------------------------------------- |
| Build, test, lint, harness commands                               | [build-test.md](build-test.md)                             |
| Swift lint, compiler, coverage, and CI gates for the iPhone app   | [swift-quality-gates.md](swift-quality-gates.md)           |
| Workspace layout & module ownership                               | [architecture.md](architecture.md)                         |
| Code style & dependencies                                         | [code-style.md](code-style.md)                             |
| Tokio / SQLite / Ollama gotchas                                   | [gotchas.md](gotchas.md)                                   |
| Git workflow & engineering standards                              | [git-workflow.md](git-workflow.md)                         |
| Event-driven portfolio and work-in-progress contract              | [improvement-drain.md](improvement-drain.md)               |
| Witness-style completion gates                                    | [witness.md](witness.md)                                   |
| PR evidence-page gate                                             | [agent-check.md](agent-check.md)                           |
| Agent skills (`/check`, `/limerick-engine`, ...)                  | [skills.md](skills.md)                                     |
| **Harness map** — what fires when, every sensor and gate          | [harness.md](harness.md)                                   |
| **Driving the live game via the limerick MCP** (QA / harness)     | [driving-the-game-via-mcp.md](driving-the-game-via-mcp.md) |
| Running CI locally with `act`                                     | [act-local.md](act-local.md)                               |
| Generated output and large-file policy                            | [repository-artifacts.md](repository-artifacts.md)         |
| Idempotency-Key support (#619)                                    | [idempotency.md](idempotency.md)                           |
| **Scaling guardrails** — per-session state, seam review checklist | [scaling-rules.md](scaling-rules.md)                       |
| Archived visual-client research and art inputs                    | [../graphics-v2-archive.md](../graphics-v2-archive.md)     |

The root `AGENTS.md` is a slim index — start there if you're new, then come here for the details.

The [rename verification record](limerick-rename-verification.md) records runtime, preservation, packaging, and platform evidence.
