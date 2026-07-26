# Notebook Person Art Pipeline

This folder contains the reviewed source inputs for the illustrated-notebook
person art slice (#1628).

## Metadata Export

The upstream art-input dataset is generated from canonical NPC/world data plus
the reviewed art-direction supplement:

```sh
cargo run --manifest-path parish/Cargo.toml -p parish-npc-tool -- art-inputs \
  --npcs mods/rundale/npcs.json \
  --world mods/rundale/world.json \
  --art-direction parish/apps/ui/art/notebook-person-art/npc-art-direction-v1.json \
  --output parish/apps/ui/art/notebook-person-art/npc-art-inputs-v1.json
```

The export covers all 23 current Rundale NPCs. The audit is
`npc-art-data-audit-v1.md`; the authoring rules are
`docs/graphics-v2/npc-portraits/art-metadata-guidelines.md`.

## Provider Candidate Generation

`generation-config-v1.json` pins the provider adapter, model snapshot, image
settings, paired-cell layout, authoritative concept reference, keyed-output
rules, validation thresholds, rate limit, retry policy, storage layout, and
initial review status. It also pins deterministic premultiplied-alpha framing
normalization for complete figures that exceed the runtime scale ceiling. The
current provider is OpenAI's
`gpt-image-2-2026-04-21` through `/v1/images/edits`. Each request attaches only
the full issue-authoritative Illustrated Parish Notebook concept and generates
both assets from one shared structured-identity prompt.

Plan the full current batch without credentials or provider calls:

```sh
npm --prefix parish/apps/ui run notebook:art-candidates
```

The current dataset plans 24 requests: one identity-locked portrait/marker pair
for each of 23 named NPCs plus one unknown-neighbour fallback pair. Narrow plans
are useful for prompt and pipeline checks:

```sh
npm --prefix parish/apps/ui run notebook:art-candidates -- \
  --npc-id 1 --asset pair --exclude-fallback
```

Live generation is opt-in and requires both `OPENAI_API_KEY` and an explicit
request ceiling. The ceiling applies after resume checks, so completed
content-addressed jobs do not consume the allowance:

```sh
npm --prefix parish/apps/ui run notebook:art-candidates -- \
  --env-file /path/to/Rundale/.env \
  --npc-id 1 --asset pair --exclude-fallback \
  --execute --max-requests 1
```

Git worktrees do not copy ignored `.env` files. In that case, pass
`--env-file` with the primary worktree's file; the runner loads it in process
without copying the key into candidate artifacts or receipts.

The runner sends one exact metadata-derived `pair_prompt`, the fixed paired
rendering contract, both full child contracts, and the sole concept reference.
One `2048x1024` response is persisted before validation, then split
deterministically into a left portrait and right marker. Each child is validated
against its own visual contract. Portrait checks cover inked drawing height,
total subject fill, dark-ink coverage, ink density, light fill, chromatic edge
coverage, and solid colored fill; marker checks cover full-body scale, width,
and safe margins. The runner removes the key and stores both transparent child
candidates. Complete, uncropped subjects that are too large are downscaled and
recentered locally with recorded before/after bounds; cropped or incomplete
figures still fail. It retries only transient provider failures, honors a global
request rate, records request ID and usage, and never records credentials.

Because antialiased dark strokes inherit magenta from the opaque provider key,
portrait postprocessing also normalizes every retained stroke to the configured
graphite ink color while preserving the generated alpha and geometry. This
postprocess revision is part of the content-addressed job identity. Marker
postprocessing preserves the watercolor palette while neutralizing
palette-forbidden magenta-balanced edge spill at any alpha; a residual-spill
gate rejects contaminated transparent candidates.

The internal schema-v4 art-direction sidecar separates stable identity
seed/cohort data from nine explicit facial-geometry dimensions, distinguishing
features, provider-facing hair prose, structured hair/headwear topology, age,
expression, wardrobe, and marker cues. Marker identity is explicitly
character-only: one person with empty hands, no contextual props or scenery,
and readability derived from hair/headwear, clothing, body shape, and stance.
The exporter rejects the legacy prop-driven schema, blank or duplicate
identities and same-cohort faces that differ in fewer than four of nine geometry
dimensions. A separate gate compares front, rear, covering, and silhouette hair
families and requires every same-cohort pair to differ in at least two.

The generated provider-input file deliberately remains schema v3. Structured
hair topology is a source-side lint contract and is not serialized into each
job record; the exact provider-facing `hair` sentence carries its rendering
requirements. This keeps unchanged paid jobs content-addressably reusable when
an internal classification or an unrelated NPC changes, which is required for
cast sizes beyond manual regeneration.
A controlled reference ablation proved that the former full-face Roisin uploads
overpowered those facts and collapsed unrelated women onto one face, so the
Roisin derivatives are retained only as review history. The full concept is now
the sole provider reference; details are in `references/README.md`.

One provider call materially improves face consistency, but stochastic image
generation is not a mathematical identity guarantee. The production guarantee
is procedural: one shared request, explicit cross-cell identity invariants,
fixed splitting, per-child validation, atomic human review, and mandatory joint
rerendering when either child or their identity match fails.

OpenAI documents that `gpt-image-2` does not currently return transparent
backgrounds. The provider raw output therefore uses flat `#ff00ff` for both
asset types; local deterministic key removal produces the transparent portrait
and marker candidates. The final portrait remains uncolored pen-and-ink and has
no baked parchment. See the official
[image-generation guide](https://developers.openai.com/api/docs/guides/image-generation)
and [model page](https://developers.openai.com/api/docs/models/gpt-image-2).

### Candidate Storage

Candidates are local authoring artifacts and are not release inputs:

```text
candidates/
  objects/<hash-prefix>/<job-sha256>/
    prompt.txt
    input-record.json
    receipt.json
    attempts/<attempt-id>/
      raw.png
      portrait-raw.png
      portrait-candidate.png
      marker-raw.png
      marker-candidate.png
      failure.json (failed attempts only)
  runs/<run-id>/
    run.json
    manifest.jsonl
```

The job hash covers the NPC/fallback record, shared pair prompt, provider/model
settings, fixed cell layout, postprocess revision, candidate index, and every
reference-image hash. A pair receipt binds the paid full sheet, both split raws,
both transparent candidates, and their hashes to one provider request. A valid
matching receipt makes reruns resumable. `--shard-count N --shard-index I`
partitions those stable job IDs for parallel workers without changing them.
Every generated receipt starts as `candidate` / `pending`, sets promotion
eligibility to false, and cannot be consumed by the approved runtime builder.
Paid attempt raws, split raws, candidates, failure records, and accepted
receipts are installed with atomic no-replace writes. A retry may reuse the
job-level prompt and input record only when their bytes are identical; it cannot
overwrite any earlier attempt. Provider refusals with no image still retain an
immutable failure record including HTTP status plus provider request ID and
structured error code/type when the provider returns them.
Execution run directories are also single-use: automatic execute IDs are
unique, and an explicit `--run-id` cannot be reused even when every job is
resumable. Resume the stable job IDs under a fresh run ID so earlier manifests
remain auditable.

Rejected raw responses are immutable and can be reprocessed after a validator
improvement without making another provider request:

```sh
npm --prefix parish/apps/ui run notebook:art-candidates -- \
  --reprocess-failure path/to/failure.json
```

Successful receipts can be migrated under a newer deterministic postprocess in
the same way. A prior generation manifest can migrate a whole batch, including
both successes and preserved failures, without provider calls:

```sh
npm --prefix parish/apps/ui run notebook:art-candidates -- \
  --reprocess-receipt path/to/receipt.json

npm --prefix parish/apps/ui run notebook:art-candidates -- \
  --reprocess-manifest path/to/manifest.jsonl \
  --run-id local-reprocess-batch
```

Every migrated receipt links to its source job and source receipt/failure,
retains the original provider request ID and usage, and returns to pending human
review. Approval never transfers implicitly across a postprocess revision.

At millions of NPCs, the same job identity and receipt contracts should move to
an object store plus queue/database index instead of one local JSON input and
filesystem tree. The current command already supplies deterministic sharding,
bounded concurrency, request caps, and idempotent resume semantics needed at
that boundary. A batch-fatal provider error such as invalid credentials, an
unknown model, exhausted quota, or a billing hard limit opens a circuit after
the bounded in-flight requests; untouched jobs remain pending for exact retry.
The local implementation does not pretend a monolithic JSON file is an
eight-million-record transport.

## Human Review Gate

Generation never sets approval. Prepare a self-contained review packet from one
or more pending candidate receipts:

```sh
npm --prefix parish/apps/ui run notebook:art-review -- prepare \
  --receipt path/to/receipt.json \
  --output path/to/review-packet \
  --packet-id review-batch-name
```

For a pair receipt, the packet embeds the full raw sheet, both split raws, both
transparent candidates, selected and tiny runtime previews, provider identity,
request ID, and every bound hash. Its one atomic checklist combines the
portrait and marker checks with cross-asset identity, correct UI/world surface
separation, a character-only marker with empty hands and no contextual props or
scenery, and acknowledgement that a failed child requires a joint rerender.
It also embeds the exact schema-v4 per-subject hair-topology vector and its
canonical digest. `prepare`, `decide`, and production promotion independently
re-read the canonical supplement and reject a changed or substituted vector;
unrelated subjects may change without invalidating this pair's paid job or
review.
The reviewer fills every checklist value with `true` or `false`, sets
`decision` to `approved` or `rejected`, identifies themselves, and records notes
for a rejection. One decision applies to both children; they cannot be approved
independently.

Submit and query the decision:

```sh
npm --prefix parish/apps/ui run notebook:art-review -- decide \
  --template path/to/completed-review.json

npm --prefix parish/apps/ui run notebook:art-review -- status \
  --receipt path/to/receipt.json
```

The decision command re-hashes the receipt, full sheet, and all four child
artifacts, rejects changed or incomplete packets, requires every check to pass
for approval, and writes an immutable review record plus a hash-linked lookup
pointer. A second decision is refused. Approval only makes both children
eligible for the later promotion stage; it does not alter
`approved-cast-v1.json` or runtime assets.

Production promotion also requires one whole-cast review binding all three
bounded visual packet manifests. This separate gate prevents individually
approved pairs from claiming that they were compared against the full cast:

```sh
npm --prefix parish/apps/ui run notebook:art-review -- prepare-cast \
  --packet path/to/named-batch-1/manifest.json \
  --packet path/to/named-batch-2/manifest.json \
  --packet path/to/named-batch-3-and-fallback/manifest.json \
  --output path/to/whole-cast-review

npm --prefix parish/apps/ui run notebook:art-review -- decide-cast \
  --template path/to/completed-whole-cast-review.json
```

The generated whole-cast HTML embeds all 48 transparent candidate images. Its
decision binds the three packet hashes plus every subject, receipt, provider raw,
portrait, marker, and per-subject topology hash, and requires
`cast_distinctive: true`.
It separately requires `cast_hair_topology_distinctive: true`, so plausible
period styling cannot collapse the women or any other cohort onto one repeated
front/rear/covering silhouette.

## Current Production Release

The 2026-07-11 named-cast run made 22 bounded provider requests and resumed the
existing Roisin request. Fourteen new pairs passed immediately; eight otherwise
valid pairs exceeded a fixed portrait or marker scale ceiling. Later deterministic
postprocess revisions migrated all 23 preserved raws locally, downscaling only
complete oversized figures, rejecting genuine edge contact, and preserving every
provider request ID and paid raw hash.

The `notebook-person-pairs-v4` catalog is rejected as a production cast. Its
pair-local checks passed, but full-cast review found eight women sharing the same
young oval face, low bun/headscarf construction, and shawl/apron template. It is
retained only as failure evidence in
`experiments/final-candidate-v4-20260712.md`; its pending review packets must not
be promoted.

The `notebook-person-pairs-v5` candidate set is also rejected: its faces were
distinct, but its women repeated a center-parted low-bun silhouette too often.
The internal art-direction catalog is now schema v4 with machine-comparable hair
front, rear, covering, and silhouette families; the provider-facing catalog stays
schema v3 so unrelated metadata changes do not invalidate paid immutable jobs.
Revision v6 proved that the topology data could diversify the cast, but failed
exact topology, sparse-ink, and several portrait-to-marker age checks. Revision
v7 tightened those contracts; after provider billing recovered, the corrected
women were generated and visually approved. That approval was superseded by the
definitive character-only marker direction because every earlier marker used the
old prop-driven schema.

The production `approved/v1` release was generated under pipeline revision
`notebook-person-pairs-v7-character-only-sparse-portraits` in the final
character-only runs. It contains 23 named pairs plus the unknown-neighbour
fallback. Every marker has empty hands and no props or scenery; every portrait is
sparse uncolored notebook ink. The user approved the complete 24-pair sheet, all
24 pair decisions and the whole-cast decision are hash-bound, and release
`41ddb06811e2bcda004421314e01560423b0986f990477c65592ac2b19576049`
is the sole production source authority.

## Approved Asset Build

```sh
npm --prefix parish/apps/ui run notebook:art-promote -- \
  --packet path/to/named-batch-1/manifest.json \
  --packet path/to/named-batch-2/manifest.json \
  --packet path/to/named-batch-3/manifest.json \
  --cast-review path/to/whole-cast-review-decision.json

npm --prefix parish/apps/ui run notebook:people

npm --prefix parish/apps/ui run notebook:art-pipeline:test
```

Promotion resolves each packet's immutable review pointers, verifies the complete
receipt/decision/prompt/config/input/reference/artifact hash chain, requires the
immutable whole-cast decision, and requires exact coverage of all 23 numeric NPC
IDs plus the fallback. It atomically writes the sole approved source authority
under `approved/v1/`; pending, rejected, incomplete, or unbound review records
cannot enter that release.

The downstream builder consumes only that checked-in approved release, so it can
run from a clean checkout without the ignored local candidate store. It verifies
the release ID, copied provenance and approval records, receipt-bound master
hashes, PNG dimensions/content/transparency, and complete roster before its first
shipping write. It contain-scales complete assets without cropping, replaces the
runtime `people/` directory, updates `asset-manifest.json` and provenance docs,
and writes dynamic PNG/HTML contact sheets for all 24 pairs.

The pipeline test command exercises generation, review, promotion, and build
contracts. Its end-to-end fixture promotes all 23 numeric NPC IDs plus fallback,
deletes the candidate store, builds the runtime pack from the approved release
alone, and requires 24 manifest entries with 48 unique emitted images.

## Review Gate

Generated candidates are not treated as approved by default. The review command
writes immutable hash-bound decisions; promotion then revalidates them and the
builder accepts only the resulting production release. `approved-cast-v1.json`
and its source sheets are retained only as legacy history and are not an approval
authority or builder input.

The source visual authority for this issue is
`docs/graphics-v2/illustrated-parish-notebook.png`. The accepted Roisin chat
portrait is an issue-produced, user-approved calibration derivative used only
to isolate that concept's sparse portrait line language. Existing unrelated
portrait experiments, marker concept sheets, old procedural busts, and
placeholder markers are not source artwork for this approved set.

## Runtime Set

The checked-in runtime pack is built from `approved/v1` and contains all 23
named NPCs plus the unknown-neighbour fallback. Portraits are emitted at
`144x164`; markers are emitted at `120x170`. `asset-manifest.json` binds every
runtime record to the approved release, source masters, and pair review, while
`person-art-contact-sheet.png` and `person-art-contact-sheet.html` provide the
visible 24-pair review artifact.
