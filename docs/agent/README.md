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

Model selection and orchestration policy belongs in the user's global
`$CODEX_HOME/AGENTS.md`. It is not duplicated here. See the
[official instruction hierarchy](https://learn.chatgpt.com/docs/agent-configuration/agents-md)
for global and repository discovery behavior.

The detailed references below describe existing implementation and tooling.

For mobile acceptance cases, use the [Phase 1 and Phase 2 test plans](../test-plans/README.md)
alongside the full product milestone requirements.

| Topic                                                             | File                                                       |
| ----------------------------------------------------------------- | ---------------------------------------------------------- |
| Build, test, lint, harness commands                               | [build-test.md](build-test.md)                             |
| Workspace layout & module ownership                               | [architecture.md](architecture.md)                         |
| Code style & dependencies                                         | [code-style.md](code-style.md)                             |
| Tokio / SQLite / Ollama gotchas                                   | [gotchas.md](gotchas.md)                                   |
| Git workflow & engineering standards                              | [git-workflow.md](git-workflow.md)                         |
| Event-driven portfolio and work-in-progress contract              | [improvement-drain.md](improvement-drain.md)               |
| Witness-style completion gates                                    | [witness.md](witness.md)                                   |
| PR proof evidence gate                                            | [agent-check.md](agent-check.md)                           |
| Agent skills (`/check`, `/limerick-engine`, ...)                  | [skills.md](skills.md)                                     |
| **Harness map** — what fires when, every sensor and gate          | [harness.md](harness.md)                                   |
| **Driving the live game via the limerick MCP** (QA / harness)     | [driving-the-game-via-mcp.md](driving-the-game-via-mcp.md) |
| Running CI locally with `act`                                     | [act-local.md](act-local.md)                               |
| Generated output and large-file policy                            | [repository-artifacts.md](repository-artifacts.md)         |
| Idempotency-Key support (#619)                                    | [idempotency.md](idempotency.md)                           |
| **Scaling guardrails** — per-session state, seam review checklist | [scaling-rules.md](scaling-rules.md)                       |
| Visual client, notebook UI, and graphics research                 | [../graphics-v2/README.md](../graphics-v2/README.md)       |

The root `CLAUDE.md` and `AGENTS.md` are slim indexes — start there if you're new, then come here for the details.

The [rename verification record](limerick-rename-verification.md) records runtime, preservation, packaging, and platform evidence.
