# Parish Endpoints Repository Guide

## Current State

- This repository contains the executable TypeScript MVP monorepo as well as its product, architecture, deployment, and decision documents.
- Treat `docs/product-vision.md` and `docs/software-architecture.md` as the product and architecture source of truth until implementation and decision records supersede specific sections. Use `docs/mvp-verification.md` for the requirement-by-requirement completion audit and evidence status.
- Repository evidence does not establish that a live deployment or provider credential smoke test succeeded; record those separately in the verification audit.
- Do not claim that build, lint, test, migration, or deployment commands exist unless their configuration is present in the repository.

## Product Boundary

- Parish Endpoints is a hosted runtime for versioned, typed AI API behavior. It is not a prompt marketplace, arbitrary-code FaaS, chatbot product, or workflow/agent platform.
- Keep Parish Engine and Parish Endpoints as separate runtime boundaries and
  deployables. In Rundale, Endpoints is a self-contained `endpoints/` pnpm
  workspace; it must not enter the Rust or player-frontend dependency graphs.
- Use **Endpoint** as the first-class product noun. Preserve the distinctions among Endpoint, mutable Endpoint Draft, immutable Endpoint Version, Deployment Alias, control-plane Parish API, and data-plane Invocation API.
- The first production dogfood target is `image -> Cottage SeedPacket JSON`, but the runtime must contain no Cottage- or seed-packet-specific behavior.

## MVP Architecture

- Build a TypeScript modular monolith with explicit module boundaries. Do not introduce microservices, Kubernetes, Redis, queues, or other distributed infrastructure without a demonstrated requirement.
- Follow the planned monorepo shape: `apps/web`, `apps/server`, and focused packages for domain, schemas, runtime, providers, auth, database, observability, and test support.
- Keep control-plane and data-plane concepts separate even if they share one deployment.
- Make the runtime depend on provider interfaces, not directly on provider SDKs. The MVP must support OpenAI and Google through adapters without leaking provider-specific concepts into public contracts.
- Use PostgreSQL as the system of record and JSON Schema as the canonical input/output contract. Support a deliberately constrained JSON Schema subset and reject unsupported constructs at publish time.
- Published Endpoint Versions are immutable. Production promotion and rollback happen through aliases; never mutate a published version in place.
- Establish organization ownership and tenant isolation from the first schema, even if the initial UI behaves as single-user.

## API and Security Invariants

- Put creator-management routes under `/api/control/v1` and authenticate them with creator identity/session authentication.
- Put public invocation routes under `/v1/endpoints/{organizationSlug}/{endpointSlug}`; consumer API keys must not authorize management operations.
- Keep provider credentials, creator instructions, and private implementation details server-side and out of consumer responses and logs.
- Store only secure hashes of invocation API keys and reveal the full key once at creation.
- Validate request sizes, image sizes, input schemas, and output schemas. Normalize provider errors at the public boundary.
- Record invocation metadata, latency, validation status, usage, and estimated provider cost without persisting raw user content by default.
- Include rate limits, model allowlists, output/token limits, request limits, and an operator kill switch in the MVP design.

## Scope Discipline

- Mobile-safe structured streaming is an approved extension for Rundale's
  dialogue Endpoint. Do not pull marketplace search, billing, payouts,
  ratings, arbitrary user code, RAG, tool calling, multi-step agents, extra
  model providers, or sophisticated evaluation infrastructure into this work.
- Prefer the simplest implementation that proves a creator can define, test, publish, invoke, observe, promote, and roll back a generic Endpoint.
- When a decision changes or refines the architecture, add a concise ADR under `docs/adr/` and update conflicting documentation in the same change.

## Development Workflow

- Before implementing, inspect the repository for the actual package manager, scripts, TypeScript settings, database tooling, and test framework; use the checked-in configuration rather than assumptions.
- Keep domain invariants and cross-module behavior behind explicit service interfaces. Do not reach into another module's tables as an implementation shortcut.
- Add tests with implementation: unit tests for domain and schema rules, adapter contract tests for providers, integration tests for persistence and HTTP boundaries, and an end-to-end invocation path for the seed-packet dogfood case.
- Use provider fakes for deterministic tests. Live-provider tests must be opt-in, cost-bounded, and skipped when credentials are absent.
- Never commit secrets, real API keys, private prompts, or customer invocation content. Keep environment examples synthetic.
- Before handing off a change, run every relevant checked-in format, lint, typecheck, test, and build command. If the repository still has no executable tooling, verify document consistency and report that no automated checks are available.

## Documentation Style

- Keep terminology consistent with the product vision and software architecture.
- Write public contracts and examples in provider-neutral terms unless documenting an adapter.
- Update examples when routes, schemas, names, or lifecycle rules change so the two core documents do not drift.
