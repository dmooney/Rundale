# Limerick engine rename verification

Rundale remains the game. The engine lives in `limerick/`, with 24
`limerick-*` packages. The headless executable is `limerick-engine`; the HTTP
client executable is `limerick`. Configuration, environment variables, MCP
registration/tools, storage identities, desktop identifiers, and current
instruction/tooling references use the new engine identity without aliases or
migration reads.

The required naming guard covers tracked paths, symlinks, hidden configuration,
and first-party text. Exact geographic exceptions retain real district and
historical vocabulary. Immutable exceptions carry SHA-256 hashes; generated
outputs have a separate exact inventory. Repeated builds and the final Linux
Docker/macOS builds produced367 byte-identical frontend files; every declared
build-input directory and configuration file is covered by a regression test. See [engine naming](engine-naming.md)
for commands and the requirements for the deferred Endpoints branch.

## Runtime and packaging

Cold MCP discovery, rejected retired tool names, a simulator new game, movement
to St. Brigid's Church, saving to `limerick_002.db`, branch reload, and session
continuity were exercised against real processes. The HTTP client and server
restart also retained the session. The MCP adapter now retains cookies and
resolves the active save path into the server's camel-case load request.
Secure-cookie replay over HTTP is confined to loopback; remote connections use
normal cookie-jar semantics. A path-alias regression test prevents an active
save from conflicting with its own advisory lock on macOS.

The Docker release image built and answered `/api/health` with HTTP 200;
protected routes retained their HTTP 401 behavior without authentication.
The desktop debug app packaged as `Rundale.app` with identifier
`ie.limerick.app`. A genuine optimized native engine binary was assembled into
a macOS arm64 smoke archive with the executable named `limerick-engine`.
No release, tag, upload, or deployment was published.

Current documentation screenshots were captured from the rendered Limerick
Designer. Captured benchmark inputs/results, provider responses, artwork
prompts/releases, and historical proof records were compared with 996 original
Git blobs: all bytes remained identical after relocation. The walkthrough's
intentional help-output change has a separate `limerick-v1` regression snapshot.

## Verification scope

The full Rust workspace built and tested on the pinned Rust 1.98 toolchain.
UI unit tests passed (946 tests across 72 files), the complete Playwright suite
passed (63 tests), and all three real Playwright-launcher integration tests
passed. Python tests passed (208, with two existing skips); Python typing,
formatting/linting, YAML, shell, TOML, workflow, proof-tooling, and repository
checks passed. Both `just check` and `just verify` passed on the pinned
toolchain with the independent proof judgment in place.

The first local Tarpaulin report failed with a macOS coverage-section error.
The required CI LLVM coverage command subsequently passed on Rust 1.98,
measuring 83.41% line coverage against the unchanged 60.8% floor.

Native checks do not establish Linux desktop packaging, Windows behavior,
physical-iPhone acceptance, or live remote inference. The bundled Python-runtime
directory is still the repository's empty directory; a working embedded model
runtime is not claimed. No paid benchmark rerun was performed.

Independent adversarial implementation review: **THUMBS UP** after the
runtime, naming, preservation, packaging, and aggregate gates were reviewed.
Required CI runs automatically on the pull request; local receipts do not claim
that remote CI has already passed.
