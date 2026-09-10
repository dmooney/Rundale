# ADR 007: Railway deployment topology

- Status: Superseded by ADR 008
- Date: 2026-08-25

## Decision

Deploy one Railway project with web, server, and managed PostgreSQL services. Build the code services from separate checked-in Dockerfiles at the monorepo root. The server binds Railway's `PORT`, runs idempotent Drizzle migrations before startup, and exposes separate liveness and database-backed readiness checks.

## Consequences

The deployment stays simple and matches the modular monolith. Provider and auth secrets are runtime variables. Next.js public variables are explicit Docker build inputs. Custom DNS is attached only after generated Railway domains pass health verification.
