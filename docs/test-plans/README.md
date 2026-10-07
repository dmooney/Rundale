# Mobile test plans

These versioned test plans complement the [mobile product specifications](../product-specs/README.md).
They preserve the imported cases and their original numbering. Changes go through
repository review, with source provenance retained in each plan.

| Phase | Test plan                                   | Scope                                                                                           |
| ----- | ------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| 1     | [Phase 1 test cases](phase-1-test-cases.md) | Fixture-only transcript/composer interaction, streaming, keyboard, accessibility, and UI scope  |
| 2     | [Phase 2 test cases](phase-2-test-cases.md) | Embedded Rust, one location/one NPC, inference, cancellation/retry, and local recovery          |
| 4     | [Phase 4 test cases](phase-4-test-cases.md) | Lifecycle, connectivity, retry, save safety, long history, accessibility, and physical sessions |

"Phase N" here, in the plan file names, and in `just mobile-verify --phase N` means
product spec Milestone N. It is not the same as the "Mobile Phase N" GitHub
milestones of the [convergence plan](../plans/mobile-engine-convergence.md).

These are test instructions, not execution reports or evidence that a milestone
has passed. Recorded results live under [`results/`](results/); the current
verdicts for Milestones 1–3 on the shared-engine line are in
[the 2026-10-04 re-acceptance](results/2026-10-04-milestones-1-3-reacceptance.md), and for Milestone 4 in
[the 2026-10-07 re-acceptance](results/2026-10-07-milestone-4-reacceptance.md). Record actual results separately, including the build/device or fixture
used, evidence, failures, and gates that were not run or could not be automated.

The plans do not replace the product's complete milestone checklists, Definition
of Done, or Quality Gate, or the technical vision's verification requirements.
Read the cases together with those contracts: composer clearing follows durable
acceptance, and production remote inference uses Limerick Endpoints. The shorter
case wording does not waive those requirements.

Run the automated gates with `just mobile-verify --phase N`; the
[mobile scripts README](../../mobile/scripts/README.md) lists its options and
report format, and [build/test](../agent/build-test.md) places it among the
other commands. Deterministic regression checks,
opt-in real-inference integration, and physical-iPhone acceptance remain separate;
passing one does not establish the others. Record physical-iPhone results as the
[Swift quality gates](../agent/swift-quality-gates.md#physical-device-acceptance)
describe.
