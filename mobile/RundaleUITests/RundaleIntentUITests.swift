import XCTest

/// Native acceptance for the interpreted-action path (#1993).
///
/// These tests launch the real application target and submit through the
/// production composer, so the assertions cover the whole
/// Swift → FFI → Rust → transport → validation/commit → semantic-event path.
/// No reducer or fixture shortcut stands in for it.
///
/// The only simulator-only branch is the Endpoint transport itself
/// (`--phase2-mock`), which returns the published Intent and Dialogue payload
/// shapes without contacting a provider. It classifies; it never selects the
/// action, resolves a target, or mutates state — the engine does all three,
/// exactly as it does against a live Endpoint.
@MainActor
final class RundaleIntentUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUp() {
        super.setUp()
        continueAfterFailure = false
        app = XCUIApplication()
    }

    override func tearDown() {
        app?.terminate()
        app = nil
        super.tearDown()
    }

    /// An input the local parser does not recognise must reach Intent and then
    /// *move the player*, with the header — which is projected from
    /// authoritative engine state, not from prose — following the commit.
    func testInferredMovementExecutesAndUpdatesTheAuthoritativeHeader() {
        launch(reset: true)
        waitForInitialScene()

        let header = app.otherElements["status.header"]
        XCTAssertTrue(header.label.localizedCaseInsensitiveContains("Kilteevan Village"))

        submit("Off to the letter office")

        // The semantic receipt names the action that actually executes.
        XCTAssertTrue(
            waitForTranscriptText("Travel to Letter Office.", timeout: 12),
            "the interpreted action should be reported before it executes"
        )
        // The authoritative transition, not a plausible NPC reply.
        XCTAssertTrue(
            waitForTranscriptText("Letter Office", timeout: 12),
            "the interpreted destination should be reached"
        )
        let moved = NSPredicate(format: "label CONTAINS[c] 'Letter Office'")
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [XCTNSPredicateExpectation(predicate: moved, object: header)],
                timeout: 12
            ),
            .completed,
            "the status header must follow the committed location"
        )
    }

    /// Ordinary conversation still works, and reaches the NPC only after the
    /// engine has interpreted the request and resolved the addressee.
    func testInferredDialogueStillReachesTheResolvedPerson() {
        launch(reset: true)
        waitForInitialScene()

        submit("Any word from beyond the parish?")

        XCTAssertTrue(
            waitForDialogue(containing: "The rain keeps", timeout: 12),
            "conversation should follow interpretation for a talk result"
        )
    }

    /// A deterministic command must settle without either Endpoint, so an
    /// offline device keeps its offline capabilities.
    func testDeterministicCommandsStayOfflineAfterTheIntentRoleExists() {
        launch(reset: true)
        waitForInitialScene()

        submit("/exits")
        XCTAssertTrue(waitForTranscriptText("Letter Office", timeout: 8))
        // The player has not moved: the deterministic command reported exits
        // rather than executing one.
        XCTAssertTrue(
            app.otherElements["status.header"].label.localizedCaseInsensitiveContains(
                "Kilteevan Village"
            )
        )
    }

    /// A failed interpretation is reported and retryable, and never leaves a
    /// half-executed action behind.
    func testFailedInterpretationIsReportedAndRetryable() {
        launch(reset: true)
        waitForInitialScene()

        submit("This will fail on purpose")

        XCTAssertTrue(
            waitForTranscriptText("retry", timeout: 12),
            "a failed interpretation should offer a retry"
        )
        XCTAssertTrue(
            app.otherElements["status.header"].label.localizedCaseInsensitiveContains(
                "Kilteevan Village"
            ),
            "a failed interpretation must not move the player"
        )
    }

    // MARK: - Helpers

    private func launch(reset: Bool) {
        app.launchArguments = [
            "--ui-tests",
            "--phase2",
            "--phase2-mock",
            "--no-auto-focus"
        ]
        if reset {
            app.launchArguments.append("--reset-fixture")
        }
        app.launch()
    }

    private func waitForInitialScene() {
        XCTAssertTrue(
            waitForTranscriptText("Morning gathers over Kilteevan", timeout: 8),
            "the embedded Rust runtime should publish its opening scene"
        )
        XCTAssertTrue(app.otherElements["status.header"].waitForExistence(timeout: 3))
    }

    private func submit(_ command: String) {
        let input = commandInput
        XCTAssertTrue(input.waitForExistence(timeout: 5))
        input.tap()
        input.typeText(command)
        app.buttons["composer.send"].tap()
        XCTAssertTrue(waitForTranscriptText(command, timeout: 8))
    }

    private var commandInput: XCUIElement {
        app.descendants(matching: .any)
            .matching(identifier: "composer.input")
            .firstMatch
    }

    private func waitForDialogue(containing text: String, timeout: TimeInterval) -> Bool {
        app.descendants(matching: .any).matching(
            NSPredicate(
                format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS 'Dialogue' AND label CONTAINS %@",
                text
            )
        ).firstMatch.waitForExistence(timeout: timeout)
    }

    private func waitForTranscriptText(_ text: String, timeout: TimeInterval) -> Bool {
        app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@", text)
        ).firstMatch.waitForExistence(timeout: timeout)
    }
}
