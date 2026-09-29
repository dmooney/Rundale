# ADR 013: Publish Endpoint definitions from files

- Status: Accepted
- Date: 2026-09-29

## Decision

An application may author its Endpoint definitions as files: one
`<slug>.v<version>.json` per immutable version, whose body is exactly an
Endpoint definition (`inputSchema`, `outputSchema`, `instructions`,
`providerConfig`, `inferenceConfig`). Rundale keeps them in its mod
(`mods/rundale/endpoints/`, Rundale ADR-025 §5). The files are the source of
truth, and the database holds published copies.

`pnpm definitions <verify|publish|export> <organization-slug> <directory>`
compares the directory with the owner's published versions of the same slugs.
A published version must have the file's canonical content hash, and its stored
content must still hash to its recorded hash. `publish` inserts each file that
has no published version under the version number in its file name, creating
the Endpoint and a Draft from the file when the slug is new. It validates new
files against the deployment's model allowlist and definition rules. It writes
nothing when any file or published copy disagrees. A published version that has
no file is an error, and `export` writes it into the directory so the files stay
complete. Aliases are not moved; promotion stays an explicit control-plane
action.

## Consequences

Definition changes are reviewed as repository diffs and ship as new version
files, never as edits to a published version. File publication shares the
version table, content hash, audit events, and immutability rules with Draft
publication. Because the file sets the version number, the control plane's
next-version rule and file publication should not both be used for the same
Endpoint. `verify` exits non-zero on any mismatch or unpublished file, so it can
gate a release.
