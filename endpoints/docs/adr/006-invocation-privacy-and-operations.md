# ADR 006: Metadata-only invocation persistence

- Status: Accepted
- Date: 2026-08-25

## Decision

Persist invocation identity, resolved version or tested draft, attempts, timing, validation, normalized usage, estimated cost, and normalized errors. Do not persist raw input, images, output, creator instructions, credentials, or provider-native payloads. Enforce byte and decoded-pixel limits, quotas, timeouts, model allowlists, output limits, rate gates, and global/provider/model/organization/Endpoint switches before inference where applicable.

## Consequences

The owner can observe reliability and usage without creating a customer-content store. In-memory rate limits are suitable for the single server replica MVP; horizontal scaling requires a shared limiter or gateway before adding replicas.
