# RundaleSession

`RundaleSession` is the session logic the iPhone app runs, kept out of the app
target so that it builds and tests on Linux and any host can run it (#2272).
It has no Combine, OSLog, UIKit, or SwiftUI dependency; the app injects
those platform pieces.

- `RundaleEngineController`: the embedded-engine session. It opens the
  engine, hydrates the transcript from snapshots, pages history, and runs the
  inference loop for each model call the engine waits on. That covers the
  Endpoint route, streamed frames, the terminal result or the failure line,
  Stop with a server-side cancel, Retry, and clarification answers.
  Credentials, the mod directory, the failure log, an Endpoint transport, and
  the engine opener are injected; `EngineRuntime` is the engine surface it
  uses, and `LimerickRuntime` conforms.
- `RundaleFixtureController`: the same surface over RundaleKit's
  deterministic fixture adapter.
- `RundaleSessionPresenter`: what the player sees and can do: header,
  transcript, draft, the composer's controls (`showsStop`, `showsRetry`,
  `canSubmitDraft`), completions, the pending clarification, accessibility
  notices, and `/bug`. `willChange` fires before each published value
  changes, so the app's `RundalePresentationModel` forwards it to
  `objectWillChange`.
- `LaunchConfiguration`, `Phase2MockEndpointTransport` (the scripted Endpoint
  for `--phase2-mock`), and the bug-report outbox.

```sh
swift test --package-path mobile/RundaleSession
```

The tests stand the C stubs from `RundaleBridge`'s
`LimerickMobileFFITestSupport` in for the Rust library and script the engine
with `ScriptedEngineRuntime`, so the Endpoint calls are the real ones and the
engine's answers are not. The app's `RundaleTests` cover the same controllers
against the real engine. A Linux run of this package is not app UI evidence.
