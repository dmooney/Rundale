# ADR 010: Atomic invocation accounting and source integrity

- Status: Accepted
- Date: 2026-09-07

## Decision

Reserve an organization's daily invocation quota in the same PostgreSQL transaction that creates the `running` invocation row. The transaction locks the organization row, counts that organization's invocations since the UTC day boundary, and inserts only when the effective quota remains available. Both public invocations and draft playground tests use this repository boundary. Invocation finalization updates only rows still in `running`, so a recovery or duplicate completion cannot replace an existing terminal result.

Deployment aliases and invocation source references use composite foreign keys that include the owning Endpoint. Invocation rows must reference exactly one immutable Endpoint Version or mutable Endpoint Draft. First-time alias promotion locks the Endpoint row before the alias compare-and-swap operation.

## Consequences

Daily quotas remain correct across concurrent requests and server replicas without Redis or an open transaction around provider execution. PostgreSQL rejects cross-Endpoint references and malformed invocation source combinations. Alias creation races resolve through the existing optimistic conflict response.
