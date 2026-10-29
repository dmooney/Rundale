# ADR-028: Drive the shared app session through a managed terminal host

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Accepted (2026-10-29), issue [#2274](https://github.com/dmooney/Rundale/issues/2274).

## Context

Agents need to test the portable parts of the iPhone app without Xcode, a
simulator, or an iOS GUI. The shared Swift controller and presenter already
run over the Rust FFI in the terminal host. Desktop MCP currently attaches to
HTTP; it does not exercise that Swift session or mobile Endpoint authorization.

## Decision

Keep one MCP implementation, `limerick-mcp`. Add a backend that owns the existing
Swift host as a child process. A versioned, private JSON-lines command protocol
separates process/session hosting from MCP handling. The default HTTP backend
and its tools remain unchanged; host mode advertises only supported actions.

The host calls the shared presenter for input, clarification, Stop, and retry.
It takes coherent presentation snapshots after presenter updates, and derives
structured output and Markdown from the same values. It adds no inference or
gameplay loop. Actions acknowledge dispatch immediately, so serial MCP requests
can inspect output or Stop while the shared controller drives inference.
Bounded recent snapshots retain provisional output for inspection.

Live authorization uses Firebase anonymous Auth REST and App Check debug-token
exchange through the existing credential interface. Tokens stay in memory;
refresh preserves the user, while restarting the host creates a new anonymous
identity. Saves contain no credentials. Temporary debug tokens are named for
the terminal/cloud session and deleted by its operator when testing ends.

Startup leaves the save unopened. Explicit new/resume actions and an exclusive
save-directory lock protect existing sessions. EOF requests Stop, persistence,
and close; the MCP parent bounds shutdown and terminates/reaps a stalled child.
A broken protocol or crashed child is reported rather than silently restarted.

## Consequences

The same host runs on macOS and Linux without Apple Firebase SDK dependencies.
Live validation can run in a Mac terminal; Linux CI separately proves build and
deterministic integration compatibility. Evidence records the actual platform
and inference mode. It proves neither app layout, accessibility, iOS lifecycle,
device attestation, simulator behavior, nor physical-iPhone acceptance.

The driver remains a development tool outside the iOS application. Package names
and game scope are unchanged; accepting a mod path does not prove game independence.
