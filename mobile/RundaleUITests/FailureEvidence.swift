import XCTest

/// Base class for every UI test. When an assertion fails it attaches the app's
/// element hierarchy and a screenshot taken at that moment, so `verify.py`
/// can copy them out of the result bundle next to the failure digest
/// (`mobile/.verification/failures/`). Subclass this, not `XCTestCase`.
class RundaleUITestCase: XCTestCase {
    private var capturingFailureEvidence = false

    override func record(_ issue: XCTIssue) {
        // UI tests run on the main actor; XCTest calls this on that thread.
        nonisolated(unsafe) let test = self
        MainActor.assumeIsolated { test.captureFailureEvidence() }
        super.record(issue)
    }

    @MainActor
    private func captureFailureEvidence() {
        // Reading the hierarchy can itself fail and re-enter record(_:).
        guard !capturingFailureEvidence else { return }
        capturingFailureEvidence = true
        defer { capturingFailureEvidence = false }
        let app = XCUIApplication()
        guard app.state != .notRunning else {
            attach(string: "The app was not running when the test failed.", name: "Failure: app state")
            return
        }
        attach(string: app.debugDescription, name: "Failure: UI hierarchy")
        let screenshot = XCTAttachment(screenshot: app.screenshot())
        screenshot.name = "Failure: screenshot"
        screenshot.lifetime = .keepAlways
        add(screenshot)
    }

    private func attach(string: String, name: String) {
        let attachment = XCTAttachment(string: string)
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
