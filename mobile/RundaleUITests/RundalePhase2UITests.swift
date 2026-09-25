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

    func testPhase2FoundationBootsCanonicalWorldAndPeig() {
        launch(reset: true)

        let header = app.otherElements["status.header"]
        XCTAssertTrue(header.waitForExistence(timeout: 8))
        XCTAssertTrue(header.label.localizedCaseInsensitiveContains("Kilteevan Village"))
        XCTAssertTrue(waitForTranscriptText("Morning gathers over Kilteevan", timeout: 8))

        submit("/look")
        XCTAssertTrue(
            waitForTranscriptItem("a muddy road, low stone walls, and smoke lifting", timeout: 8),
            "The assertion must observe the newly correlated /look result, not the pre-existing header"
        )
        XCTAssertFalse(app.buttons["composer.stop"].exists, "A local action must not open Endpoint work")
        submit("/people")
        XCTAssertTrue(waitForTranscriptText("Peig", timeout: 8))
        XCTAssertFalse(app.buttons["composer.stop"].exists, "Every deterministic Phase 2 action stays offline")
    }

    func testPhase2LookBatchClearsComposerAfterRustAcceptance() {
        launch(reset: true)
        waitForInitialScene()

        let input = commandInput
        input.tap()
        input.typeText("/look")
        app.buttons["composer.send"].tap()

        XCTAssertTrue(waitForTranscriptText("Kilteevan Village", timeout: 8))
        XCTAssertTrue(waitForValue("", on: input, timeout: 8))
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 3))
    }

    func testSimulatorReturnKeySubmitsDraft() throws {
#if !targetEnvironment(simulator)
        throw XCTSkip("The simulator return-key contract does not apply to a physical iPhone")
#else
        launch(reset: true, simulatorReturnKey: true)
        waitForInitialScene()

        let input = commandInput
        input.tap()
        input.typeText("/look\n")

        XCTAssertTrue(waitForTranscriptItem("/look", timeout: 8))
        XCTAssertTrue(waitForValue("", on: input, timeout: 8))
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 3))
#endif
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
        XCTAssertTrue(waitForDialogue(containing: "A worn sign leans by the gate", timeout: 12))
        XCTAssertTrue(provisional.label.contains("In progress"))
        XCTAssertTrue(waitForDialogue(containing: "stands beyond the alder trees", timeout: 12))
        let completed = app.descendants(matching: .any)
            .matching(identifier: rowID)
            .firstMatch
        XCTAssertTrue(completed.waitForExistence(timeout: 5))
        XCTAssertFalse(completed.label.contains("In progress"))
        XCTAssertFalse(completed.label.contains("A worn sign leans by the gate"))
        XCTAssertTrue(completed.label.contains("stands beyond the alder trees"))
    }

    func testPhase2EndpointErrorNamesTheFailureCategory() {
        launch(reset: true)
        waitForInitialScene()

        submit("ask Peig to fail")

        XCTAssertTrue(
            waitForTranscriptText(
                "The storyteller service is temporarily unavailable. You can retry this request.",
                timeout: 8
            )
        )
        XCTAssertFalse(waitForTranscriptText("The response could not be validated.", timeout: 1))
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
        XCTAssertEqual(completedDialogue.count, 0)

        // The slow fixture would finish after six seconds if cancellation or
        // attempt terminality were ineffective. Wait beyond that boundary and
        // then reopen SQLite before allowing a retry.
        XCTAssertEqual(
            XCTWaiter.wait(for: [XCTestExpectation(description: "late completion window")], timeout: 7),
            .timedOut
        )
        XCTAssertEqual(completedDialogue.count, 0, "A stopped attempt must not commit after its delayed completion")
        app.terminate()
        app = XCUIApplication()
        launch(reset: false)
        XCTAssertEqual(completedDialogue.count, 0, "A stopped attempt must remain uncommitted after SQLite reopen")
        XCTAssertEqual(rows(containing: "Interrupted; not applied").count, 1)
        assertSingleTranscriptItem(containing: "ask Peig about the church slowly")

        let retry = app.buttons["composer.retry"]
        XCTAssertTrue(retry.waitForExistence(timeout: 8))
        retry.tap()

        XCTAssertTrue(waitForDialogue(containing: "The rain keeps the old road quiet", timeout: 12))
        XCTAssertTrue(waitForDialogue(containing: "stands beyond the alder trees", timeout: 20))
        XCTAssertTrue(waitForTranscriptText("Interrupted; not applied", timeout: 8))
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
        XCTAssertFalse(app.buttons["composer.retry"].exists)
        XCTAssertEqual(completedDialogue.count, 1)
    }

    func testPhase2SQLiteRestoresCommittedDialogueAfterRelaunch() {
        launch(reset: true)
        waitForInitialScene()

        let command = "ask Peig about the church"
        submit(command)
        XCTAssertTrue(waitForDialogue(containing: "stands beyond the alder trees", timeout: 12))
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))

        let commandID = rows(containing: command).firstMatch.identifier
        let dialogueID = completedDialogue.firstMatch.identifier
        XCTAssertFalse(commandID.isEmpty)
        XCTAssertFalse(dialogueID.isEmpty)

        app.terminate()
        app = XCUIApplication()
        launch(reset: false)

        XCTAssertTrue(app.otherElements["status.header"].waitForExistence(timeout: 8))
        XCTAssertTrue(waitForTranscriptText(command, timeout: 12))
        XCTAssertTrue(waitForDialogue(containing: "The rain keeps the old road quiet", timeout: 12))
        XCTAssertTrue(waitForDialogue(containing: "stands beyond the alder trees", timeout: 12))
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
        XCTAssertEqual(rows(containing: command).count, 1)
        XCTAssertEqual(completedDialogue.count, 1)
        XCTAssertEqual(rows(containing: command).firstMatch.identifier, commandID)
        XCTAssertEqual(completedDialogue.firstMatch.identifier, dialogueID)

        app.terminate()
        app = XCUIApplication()
        launch(reset: false)
        XCTAssertEqual(rows(containing: command).count, 1, "Repeated restoration must not duplicate the command")
        XCTAssertEqual(completedDialogue.count, 1, "Repeated restoration must not duplicate the committed response")
        XCTAssertEqual(rows(containing: command).firstMatch.identifier, commandID)
        XCTAssertEqual(completedDialogue.firstMatch.identifier, dialogueID)
    }

    private func launch(reset: Bool, simulatorReturnKey: Bool = false) {
        app.launchArguments = [
            "--ui-tests",
            "--phase2",
            "--phase2-mock",
            "--no-auto-focus"
        ]
        if reset {
            app.launchArguments.append("--reset-fixture")
        }
        if simulatorReturnKey {
            app.launchArguments.append("--simulator-return-key")
        }
        app.launch()
    }

    private func waitForInitialScene() {
        XCTAssertTrue(
            waitForTranscriptText("Morning gathers over Kilteevan", timeout: 8),
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

    private func waitForTranscriptItem(_ text: String, timeout: TimeInterval) -> Bool {
        app.descendants(matching: .any).matching(
            NSPredicate(
                format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS %@",
                text
            )
        ).firstMatch.waitForExistence(timeout: timeout)
    }

    private var completedDialogue: XCUIElementQuery {
        app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS 'Dialogue' AND label CONTAINS 'stands beyond the alder trees' AND NOT label CONTAINS 'In progress' AND NOT label CONTAINS 'Interrupted'"
        ))
    }

    private func rows(containing text: String) -> XCUIElementQuery {
        app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS %@", text
        ))
    }

    private func assertSingleTranscriptItem(containing text: String,
                                            file: StaticString = #filePath,
                                            line: UInt = #line) {
        XCTAssertEqual(rows(containing: text).count, 1, file: file, line: line)
    }

    private func waitForValue(_ value: String,
                              on element: XCUIElement,
                              timeout: TimeInterval) -> Bool {
        let predicate = value.isEmpty
            ? NSPredicate(format: "value == '' OR value == nil")
            : NSPredicate(format: "value == %@", value)
        let expectation = XCTNSPredicateExpectation(predicate: predicate, object: element)
        return XCTWaiter.wait(for: [expectation], timeout: timeout) == .completed
    }
}
