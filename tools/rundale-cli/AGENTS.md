# tools/rundale-cli — agent scope

The Rundale CLI that plays the app's session as Markdown frames. Start with
[README.md](README.md).

## Commands

```sh
just rundale-cli-test                              # golden frames (CI: Rundale CLI golden frames)
RUNDALE_UPDATE_GOLDENS=1 just rundale-cli-test     # rewrite goldens, then review the diff
cd tools/rundale-cli && ../../mobile/.build/swift-tools/bin/swiftlint lint --strict
```

## Traps

- **Fixture resume validates the session, not the draft.** Its authoritative
  snapshot is `phase1-session.json`; `projection.json` stores only the draft.
  Validate the snapshot before controller recovery can create fresh state.
- **A behaviour change belongs in `mobile/RundaleSession`, not here.** The
  host only sets the draft and taps what a view would, then renders the
  presenter. Logic added here would drift from the app (#2272).
- **The host lives outside `mobile/` on purpose.** It is not an app input
  under `verify.py`'s `ATTESTATION_PATHSPECS`, so changing it needs no
  `mobile-verify` pass. A change to `RundaleSession`, the mod, or the engine
  still does.
- **Goldens depend on the mock's timing, not wall-clock races.** A streaming
  frame needs the reply text to hold still for 0.5 s; the mock pauses 2 s
  between deltas and sends the final right after the last one, which is
  therefore never printed. Shortening the mock's pauses below the threshold
  removes streaming frames from the goldens.
- **SwiftLint needs this directory's `.swiftlint.yml`.** Run with the mobile
  config directly, its `included` list skips these sources and reports
  nothing.

- **Pipe reads must return before EOF.** Use available-byte reads for the private
  command protocol. Foundation `read(upToCount:)` on macOS can wait for the
  requested buffer to fill, deadlocking an MCP client that waits for a response
  before writing its next line. The cross-language MCP test keeps stdin open.

- **Swiftdansi 0.3.1 does not apply `NO_COLOR` to text styles itself.** Pass
  `RenderOptions.color` explicitly at the executable boundary; keep protocol and
  golden-frame rendering as raw Markdown. The real PTY tests cover both paths.
