import XCTest

extension XCTestCase {
    /// Pauses for a human viewer when recording a demo; a no-op in normal
    /// runs. Pass `TEST_RUNNER_RUNDALE_DEMO_HOLD=<seconds>` to `xcodebuild
    /// test` (xcodebuild strips the prefix); each hold then lasts the longer
    /// of `seconds` and that value. See the phase demo plan's recordings.
    func hold(_ seconds: TimeInterval) {
        guard let value = ProcessInfo.processInfo.environment["RUNDALE_DEMO_HOLD"],
              let minimum = TimeInterval(value), minimum > 0 else { return }
        Thread.sleep(forTimeInterval: max(seconds, minimum))
    }
}
