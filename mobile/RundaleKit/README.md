# RundaleKit

`RundaleKit` is the Foundation-only presentation contract for the Phase 1
native player. The package contains semantic event values, stable identities,
the reducer that projects events into transcript/session state, deterministic
fixture playback, completion data, and explicit-path fixture persistence.

The package deliberately has no SwiftUI, network, Rust, or game simulation
dependency. A native client observes `PresentationSession.state` (or wraps the
reducer in its own `ObservableObject`) and subscribes to any `SessionAdapter`.

## Fixture flow

```swift
let adapter = FixtureSessionAdapter(script: .phase1)
let sessionID = await adapter.sessionID
let presentation = await MainActor.run {
    PresentationSession(state: SessionState(sessionID: sessionID))
}

let draft = Draft(text: "ask Peig about the old church")
let stream = await adapter.events(after: nil)
_ = try await adapter.submit(
    text: "ask Peig about the old church",
    draftID: draft.id,
    logicalRequestID: nil
)

// Fixture playback is manual, so advance it explicitly in the example.
_ = await adapter.runUntilFinished()
await adapter.finishEventStream()

do {
    for try await event in stream {
        await MainActor.run { presentation.apply(event) }
    }
} catch {
    // Reconcile from the adapter journal before resubscribing.
}
```

Live event streams retain at most 256 pending events. A stalled consumer gets
an explicit overflow error so its owner can refresh from authoritative session
state instead of silently losing semantic events.

`FixtureSessionAdapter.step()` advances exactly one scripted event. It never
sleeps, calls a model, or mutates a simulated world, so cancellation, retry,
clarification, repeated chunks, and late callbacks are reproducible in tests.

The `.playerCommand` acceptance event carries the submitted `sourceDraftID`.
The reducer clears the draft only when that identity and the submitted text
still match the current draft. Text entered while acceptance or streaming is
in flight therefore survives.

`FixtureSessionStore(fileURL:)` and `FixtureDraftStore(fileURL:)` require their
paths explicitly and use Foundation's atomic write option. Decode and contract
version failures happen before any write, leaving the original bytes available
for recovery or a compatibility message.
