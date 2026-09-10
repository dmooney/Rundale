import XCTest

/// End-to-end checks for the embedded Parish runtime.  These tests deliberately
/// launch the real application target and use only the deterministic Endpoint
/// transport that is compiled behind the Phase 2 UI-test arguments.
@MainActor
final class RundalePhase2UITests: XCTestCase {
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

    func testPhase2BootShowsCrossroadsAndPeig() {
        launch(reset: true)

        let header = app.otherElements["status.header"]
        XCTAssertTrue(header.waitForExistence(timeout: 8))
        XCTAssertTrue(header.label.localizedCaseInsensitiveContains("crossroads"))
        XCTAssertTrue(waitForTranscriptText("Rain darkens the road", timeout: 8))

        submit("/look")
        XCTAssertTrue(waitForTranscriptText("Crossroads · stone wall · lane to the east", timeout: 8))
        submit("/people")
        XCTAssertTrue(waitForTranscriptText("Peig", timeout: 8))
    }

    func testPhase2LookBatchClearsComposerAfterRustAcceptance() {
        launch(reset: true)
        waitForInitialScene()

        let input = commandInput
        input.tap()
        input.typeText("/look")
        app.buttons["composer.send"].tap()

        XCTAssertTrue(waitForTranscriptText("Crossroads · stone wall · lane to the east", timeout: 8))
        XCTAssertTrue(waitForValue("", on: input, timeout: 8))
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 3))
    }

    func testPhase2CompletionsUseRustNearbyPeople() {
        launch(reset: true)
        waitForInitialScene()

        let input = commandInput
        XCTAssertTrue(input.waitForExistence(timeout: 5))
        input.tap()
        input.typeText("ask @")

        XCTAssertTrue(app.buttons["completion.npc-peig"].waitForExistence(timeout: 8))
        XCTAssertFalse(app.buttons["completion.npc-micheal"].exists)
        XCTAssertFalse(app.buttons["completion.npc-roisin"].exists)
    }

    func testPhase2IncrementalChunksUpdateOneDialogueRowBeforeFinal() {
        launch(reset: true)
        waitForInitialScene()

        submit("ask Peig about the church")

        let provisional = dialogueRow(containing: "The rain keeps")
        XCTAssertTrue(provisional.waitForExistence(timeout: 8))
        let rowID = provisional.identifier
        XCTAssertFalse(rowID.isEmpty)
        XCTAssertTrue(provisional.label.contains("In progress"))

        XCTAssertTrue(waitForDialogue(containing: "the old road quiet", timeout: 12))
        XCTAssertTrue(waitForDialogue(containing: "stands beyond the alder trees", timeout: 12))
        let completed = app.descendants(matching: .any)
            .matching(identifier: rowID)
            .firstMatch
        XCTAssertTrue(completed.waitForExistence(timeout: 5))
        XCTAssertFalse(completed.label.contains("In progress"))
    }

    func testPhase2StopLeavesInterruptedAttemptAndRetryCompletes() {
        launch(reset: true)
        waitForInitialScene()

        submit("ask Peig about the church slowly")
        let stop = app.buttons["composer.stop"]
        XCTAssertTrue(stop.waitForExistence(timeout: 8))
        stop.tap()

        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
        XCTAssertTrue(waitForTranscriptText("Interrupted; not applied", timeout: 8))
        let retry = app.buttons["composer.retry"]
        XCTAssertTrue(retry.waitForExistence(timeout: 8))
        retry.tap()

        XCTAssertTrue(waitForDialogue(containing: "The rain keeps the old road quiet", timeout: 12))
        XCTAssertTrue(waitForDialogue(containing: "stands beyond the alder trees", timeout: 20))
        XCTAssertTrue(waitForTranscriptText("Interrupted; not applied", timeout: 8))
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
        XCTAssertFalse(app.buttons["composer.retry"].exists)
    }

    func testPhase2SQLiteRestoresCommittedDialogueAfterRelaunch() {
        launch(reset: true)
        waitForInitialScene()

        let command = "ask Peig about the church"
        submit(command)
        XCTAssertTrue(waitForDialogue(containing: "stands beyond the alder trees", timeout: 12))
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))

        app.terminate()
        app = XCUIApplication()
        launch(reset: false)

        XCTAssertTrue(app.otherElements["status.header"].waitForExistence(timeout: 8))
        XCTAssertTrue(waitForTranscriptText(command, timeout: 12))
        XCTAssertTrue(waitForDialogue(containing: "The rain keeps the old road quiet", timeout: 12))
        XCTAssertTrue(waitForDialogue(containing: "stands beyond the alder trees", timeout: 12))
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
    }

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
            waitForTranscriptText("Rain darkens the road", timeout: 8),
            "The embedded Rust runtime should publish its opening scene"
        )
        XCTAssertTrue(
            app.otherElements["status.header"].waitForExistence(timeout: 3),
            "The Phase 2 header should be present"
        )
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

    private func dialogueRow(containing text: String) -> XCUIElement {
        app.descendants(matching: .any).matching(
            NSPredicate(
                format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS 'Dialogue' AND label CONTAINS %@",
                text
            )
        ).firstMatch
    }

    private func waitForDialogue(containing text: String, timeout: TimeInterval) -> Bool {
        dialogueRow(containing: text).waitForExistence(timeout: timeout)
    }

    private func waitForTranscriptText(_ text: String, timeout: TimeInterval) -> Bool {
        app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@", text)
        ).firstMatch.waitForExistence(timeout: timeout)
    }

    private func waitForValue(_ value: String,
                              on element: XCUIElement,
                              timeout: TimeInterval) -> Bool {
        let predicate = NSPredicate(format: "value == %@", value)
        let expectation = XCTNSPredicateExpectation(predicate: predicate, object: element)
        return XCTWaiter.wait(for: [expectation], timeout: timeout) == .completed
    }
}
