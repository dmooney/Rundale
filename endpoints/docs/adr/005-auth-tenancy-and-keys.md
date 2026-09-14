# ADR 005: Owner identity, tenant isolation, and consumer keys

- Status: Accepted
- Date: 2026-08-25
- Authentication decision superseded by: ADR 009

## Decision

Use managed identity sessions for creator authentication in production and authorize only the configured owner user. Every creator query is organization-scoped from the first schema. Use separate high-entropy consumer API keys with Endpoint scopes; store only SHA-256 digests and a non-secret prefix, and reveal the full key once. ADR 009 selects Firebase Authentication for the managed identity implementation.

## Consequences

Consumer credentials cannot authorize management operations. Cross-organization Endpoint resolution fails as not found. Development identity headers exist only in explicit non-production mode.
