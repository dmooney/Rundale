# ADR 011: Nullable and bounded-array schema extension

- Status: Accepted
- Date: 2026-09-07
- Refines: [ADR 002: Constrained JSON Schema contracts](002-json-schema-contracts.md)

## Context

The deliberately small JSON Schema subset was sufficient for simple typed output, but the current generic image Endpoint contract also needs nullable scalar and object fields plus bounded arrays. Existing contracts may express nullability with a direct type array such as `["string", "null"]`; that form remains part of the ADR 002 subset. Supporting unrestricted JSON Schema composition would expand provider translation, validation, and UI behavior beyond the MVP.

## Decision

Extend the provider-neutral subset with two constrained forms while preserving direct type arrays:

- A nullable node may use `anyOf` with exactly two branches: one supported typed branch and one `{ "type": "null" }` branch. The nullable `anyOf` node cannot also declare `type`; all branches remain subject to the existing subset rules. This constrained `anyOf` form complements, and does not replace, a direct type array such as `["integer", "null"]`.
- An array may use `maxItems` when it is a non-negative safe integer.

Direct type arrays remain valid only when every member is a supported type. Reject every other composition form, malformed nullable union, reference, or invalid limit. Compile accepted schemas with Ajv and remove caller-supplied schema objects from its cache because this subset has no cross-schema references. The runtime remains generic and does not contain SeedPacket or Cottage-specific schema logic.

## Consequences

Current and future Endpoint contracts can express nullable values through either a supported direct type array or the constrained `anyOf` form, plus bounded collections, while retaining predictable provider translation and independent output validation. The subset remains intentionally narrower than full JSON Schema; adding another construct requires a separate decision and tests.
