import Foundation

/// Presentation pause for screen recordings. A no-op unless
/// `TEST_RUNNER_RUNDALE_DEMO_HOLD=<seconds>` is set on `xcodebuild test`
/// (xcodebuild strips the `TEST_RUNNER_` prefix, so the test runner reads
/// `RUNDALE_DEMO_HOLD`). The pause is `max(seconds, that value)`. Use it only
/// for pacing a human viewer; never for waiting on app state.
func hold(_ seconds: TimeInterval) {
    guard let value = ProcessInfo.processInfo.environment["RUNDALE_DEMO_HOLD"],
          let minimum = TimeInterval(value), minimum > 0 else { return }
    Thread.sleep(forTimeInterval: max(seconds, minimum))
}
