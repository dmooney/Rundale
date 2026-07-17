# Visible Hair and Sparse Ink Candidate Revision v7

Date: 2026-07-14

**Status: provider-blocked before image creation. No v7 art exists yet.**

Revision v7 is the corrected ten-woman retry after v6's exact-topology,
sparse-ink, and portrait-to-marker age failures. It keeps the 14 unaffected
content-addressed pairs reusable and changes only the ten women's own input
records and job IDs.

## Canonical Inputs

- Pipeline revision: `notebook-person-pairs-v5`
- Provider: OpenAI Images edits adapter
- Model snapshot: `gpt-image-2-2026-04-21`
- Request shape: ten atomic `2048x1024` portrait-marker pairs
- Sole provider reference: `docs/graphics-v2/illustrated-parish-notebook.png`
- Generation config SHA-256: `467aa86d7823ea878aeacb584592a293bbed8b32fdba560d59ff181c2ac100e6`
- NPC art-input dataset SHA-256: `6d930a2cf4417b9f86b2dd518584b1935d0d53e25be72e93710f656d1e614274`

The complete plan reports 24 jobs, 14 resumable existing pairs, and ten pending
provider requests. A focused retry plan reports the same ten pending job IDs.

## Provider Attempt

Run `identity-v7-visible-hair-style-20260714` attempted exactly ten requests.
Every request returned HTTP 400 with OpenAI error code
`billing_hard_limit_reached` before an image response was created. The run
finished with 0 generated, 0 resumed, and 10 failed attempts. Each failure is
retained immutably beneath its content-addressed job object; no raw image or
candidate receipt is claimed for these attempts.

The failed attempts do not poison resumption. Dry run
`identity-v7-retry-plan-20260714` still reports all ten as pending, so the exact
same bounded selection can resume once provider billing is restored. Execution
must use a fresh run ID; run manifests are single-use and immutable even though
the ten content-addressed job IDs remain unchanged.

After the runner gained batch-fatal circuit breaking and provider-error request
provenance, one bounded canary retried Siobhan under run
`identity-v7-billing-canary-20260714`. It received the same 400/code before
image creation, captured OpenAI request ID
`req_75799d07481d4f0b91f5addfb0278abc`, marked the error `batch_fatal: true`,
and made no further requests. All ten corrected jobs remain pending.

A later single-request canary, run
`identity-v7-billing-canary-20260714-2`, tested the fully hardened runner without
sweeping the queue. It also received HTTP 400 with
`billing_hard_limit_reached` before image creation and captured OpenAI request ID
`req_e8b90489cdbc4d06b0ab8924a042ea9a`. The attempt is retained immutably; all
ten corrected jobs are still retryable and pending.

## Remaining Gate

After generation, the ten pairs must pass independent review for exact declared
hair topology, sparse uncolored portrait ink, restrained marker watercolor,
same face/apparent age across cells, period fit, framing, and full-cast identity.
They must then be shown beside the 14 unchanged candidates. No v7 pair may be
approved or promoted until the user explicitly approves those exact displayed
hashes and both pair-level and whole-cast immutable decisions are written.
