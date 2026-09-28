# ADR 002: Constrained JSON Schema contracts

- Status: Accepted
- Date: 2026-08-25

## Decision

Use JSON Schema as the canonical Endpoint input and output contract, compiled with Ajv. Accept only the explicitly supported subset and the `x-semantic-type: image` extension. Reject unsupported keywords and invalid definitions before publish, and independently validate every successful model output.

The constrained subset supports object, array, string, integer, number, boolean, and null types; properties, required, items, additionalProperties, enum, string length limits, numeric limits, and `maxItems`. A schema node may declare one supported type or a non-empty array of supported types, including the direct nullable form `["string", "null"]`. The constrained `anyOf` nullable form is limited to exactly two branches: one supported typed schema and one `{ "type": "null" }` schema. Other composition and reference keywords remain unsupported.

`maxItems` must be a non-negative safe integer. Accepted schemas are compiled for validation and then removed from Ajv's object-keyed schema cache because this subset contains no references between caller-supplied schemas; this keeps repeated draft or invocation compilation from retaining an unbounded number of schema objects.

The `x-semantic-type: image` extension is allowed on at most one direct property of a top-level object schema. Nested, array-item, root-level, and nullable-branch image fields are rejected because the MVP multipart boundary exposes one named image attachment and the request parser resolves only top-level properties.

The rationale and boundary for the nullable and bounded-array extension are recorded in [ADR 011](011-nullable-bounded-schema-extension.md).

## Consequences

Public contracts remain provider-neutral and can later drive generated clients. Images travel as multipart binary data and appear to schema validation as their semantic field; the runtime does not invent application-specific types.
