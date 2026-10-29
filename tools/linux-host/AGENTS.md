# tools/linux-host — agent scope

The Linux host that plays the app's session as Markdown frames. Start with
[README.md](README.md).

## Commands

```sh
just linux-host-test                              # golden frames (CI: Linux host golden frames)
RUNDALE_UPDATE_GOLDENS=1 just linux-host-test     # rewrite goldens, then review the diff
cd tools/linux-host && ../../mobile/.build/swift-tools/bin/swiftlint lint --strict
```

## Traps

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
