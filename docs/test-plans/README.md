# Mobile test plans

These versioned test plans complement the [mobile product specifications](../product-specs/README.md).
They preserve the imported cases and their original numbering. Changes go through
repository review, with source provenance retained in each plan.

| Phase | Test plan                                   | Scope                                                                                          |
| ----- | ------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| 1     | [Phase 1 test cases](phase-1-test-cases.md) | Fixture-only transcript/composer interaction, streaming, keyboard, accessibility, and UI scope |
| 2     | [Phase 2 test cases](phase-2-test-cases.md) | Embedded Rust, one location/one NPC, inference, cancellation/retry, and local recovery         |

These are test instructions, not execution reports or evidence that a milestone
has passed. Record actual results separately, including the build/device or fixture
used, evidence, failures, and gates that were not run or could not be automated.

The plans do not replace the product's complete milestone checklists, Definition
of Done, or Quality Gate, or the technical vision's verification requirements.
Read the cases together with those contracts: composer clearing follows durable
acceptance, and production remote inference uses Limerick Endpoints. The shorter
case wording does not waive those requirements.

Use [build/test](../agent/build-test.md) to distinguish existing commands from
the required mobile verification entry point. Deterministic regression checks,
opt-in real-inference integration, and physical-iPhone acceptance remain separate;
passing one does not establish the others.
