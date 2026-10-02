# Generated Art and Rendering Rules

[Character-art identity](#character-art-identity),
[generated-art visual contracts](#generated-art-visual-contracts), and
[character markers](#character-markers) are specialized to the existing
generated character-art pipeline. When maintaining that pipeline, they apply in
full; they do not turn mobile feature work into an art-generation requirement.
[Artifact content validation](#artifact-content-validation),
[billable artifact persistence](#billable-artifact-persistence),
[generated-file transactions](#generated-file-transactions), and
[production rendering support assets](#production-rendering-support-assets)
apply whenever the corresponding artifact, generated-file, or production
rendering-support behavior exists, including future mobile surfaces. They are
cross-cutting requirements and are not waived by the character-art scope. See
[engineering-rules.md](engineering-rules.md) for the complete rule index.

<a id="artifact-content-validation"></a>
<a id="rule-14"></a>

## Validate artifact content, not just the envelope

A handler returning a produced artifact (screenshot, export, render, generated
file) must verify real content before reporting success — a blank/degenerate
result is an `Err`, never "nonzero bytes = success". Pattern:
`reject_blank_capture` in
`limerick/crates/limerick-tauri/src/commands/screenshot.rs` (#1301).

<a id="character-art-identity"></a>
<a id="rule-20"></a>

## Character-art identity must be distinct across the cast

Encode stable structured facial geometry separately from age, affect,
hair/headwear topology, wardrobe, and props. Hair/headwear must expose
machine-comparable front, rear, covering, and overall-silhouette families;
reject missing, duplicate, or near-duplicate facial vectors and repeated
hairstyle topology within relevant cohorts before generation. A pair matching
itself is not enough: shared full-face style references must not become an
identity prior for unrelated characters, and approval must compare each result
against the full cast.

<a id="billable-artifact-persistence"></a>
<a id="rule-21"></a>

## Persist billable external artifacts before validating them

When an external API returns a non-deterministic generated artifact, write the
raw bytes, content hash, provider request ID, and source provenance before
content validation. Store each attempt immutably: a rejection must retain that
raw artifact and link it from a failure receipt, and a retry must never
overwrite an earlier paid response.

<a id="generated-art-visual-contracts"></a>
<a id="rule-22"></a>

## Validate generated art against the asset-specific visual contract

File format, dimensions, and nonblank pixels are necessary but insufficient.
Keep portrait, marker, scene, and UI-art prompts/references separate; encode
machine-checkable composition and style signals where practical (for example
bounds, fill, ink density, or palette), retain human review for semantic
judgment, and test that a representative wrong-style artifact is rejected.

<a id="character-markers"></a>
<a id="rule-23"></a>

## Character markers are character-only cutouts

Make marker identity readable from face, hair/headwear, clothing, body shape,
and stance. Reject held or carried objects, extra people, furniture,
architecture, vegetation, scenery fragments, ground planes, and shadows unless
an issue explicitly opts into contextual markers; worn clothing and headwear
remain valid identity cues.

<a id="generated-file-transactions"></a>
<a id="rule-24"></a>

## Commit generated files as transactions

Finish every fallible preparation and handled-failure cleanup before the final
source-snapshot comparison, then perform the same-filesystem rename
immediately. Inject source mutation at the last cleanup seam and exercise
competing snapshots across processes.

<a id="production-rendering-support-assets"></a>
<a id="rule-41"></a>

## Bundle production rendering support assets

Never ship required fonts, glyphs, sprites, or equivalent UI resources from
demo or best-effort endpoints. Serve them from the frontend distribution
unless an owned production SLA is documented, keep CSP origins minimal, and
test both packaged integrity and browser requests/errors across every
consuming surface.
