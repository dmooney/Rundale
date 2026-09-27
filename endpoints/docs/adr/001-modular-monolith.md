# ADR 001: TypeScript modular monolith

- Status: Accepted
- Date: 2026-08-25

## Decision

Build Parish Endpoints as a pnpm TypeScript monorepo with `apps/web`, `apps/server`, and focused domain, schema, runtime, provider, auth, database, observability, and test-support packages. Deploy the web and server as separate Cloud Run services backed by one dedicated Cloud SQL PostgreSQL system of record, while retaining explicit control-plane and data-plane module boundaries inside the server.

## Consequences

The MVP avoids distributed coordination, queues, Kubernetes, Redis, and microservice operational overhead. Package interfaces enforce boundaries and allow later extraction only when measured requirements justify it.
