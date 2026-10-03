import XCTest

/// End-to-end checks for the embedded Limerick runtime.  These tests deliberately
/// launch the real application target and use only the deterministic Endpoint
/// transport that is compiled behind the Phase 2 UI-test arguments.
@MainActor
class RundalePhase2UITestCase: XCTestCase {
    var app: XCUIApplication!

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

    /// The opening scene on the canonical world (`mods/rundale`).
    static let opening = "A muddy road runs between low stone walls"

    /// Mícheál and Róisín are at home in the morning. Movement is parsed
    /// locally, so no Endpoint is called.
    func goToTheCottage() {
        submit("go to Connolly Cottage")
        XCTAssertTrue(waitForTranscriptItem("A peat fire warms the single room", timeout: 8))
    }

    func launch(reset: Bool, simulatorReturnKey: Bool = false) {
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

    func waitForInitialScene() {
        XCTAssertTrue(
            waitForTranscriptText(Self.opening, timeout: 8),
            "The embedded Rust runtime should publish its opening scene"
        )
        XCTAssertTrue(
            app.otherElements["status.header"].waitForExistence(timeout: 3),
            "The Phase 2 header should be present"
        )
    }

    func submit(_ command: String) {
        let input = commandInput
        XCTAssertTrue(input.waitForExistence(timeout: 5))
        input.tap()
        input.typeText(command)
        app.buttons["composer.send"].tap()
        // Acceptance clears the draft. The command row itself can scroll out
        // of the lazy transcript under a long scene, so it is not the signal.
        XCTAssertTrue(waitForValue("", on: input, timeout: 8), command)
    }

    var commandInput: XCUIElement {
        app.descendants(matching: .any)
            .matching(identifier: "composer.input")
            .firstMatch
    }

    func dialogueRow(containing text: String) -> XCUIElement {
        app.descendants(matching: .any).matching(
            NSPredicate(
                format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS 'Dialogue' AND label CONTAINS %@",
                text
            )
        ).firstMatch
    }

    func waitForDialogue(containing text: String, timeout: TimeInterval) -> Bool {
        dialogueRow(containing: text).waitForExistence(timeout: timeout)
    }

    func waitForTranscriptText(_ text: String, timeout: TimeInterval) -> Bool {
        app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@", text)
        ).firstMatch.waitForExistence(timeout: timeout)
    }

    func waitForTranscriptItem(_ text: String, timeout: TimeInterval) -> Bool {
        app.descendants(matching: .any).matching(
            NSPredicate(
                format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS %@",
                text
            )
        ).firstMatch.waitForExistence(timeout: timeout)
    }

    func waitForValue(_ value: String,
                              on element: XCUIElement,
                              timeout: TimeInterval) -> Bool {
        let predicate = value.isEmpty
            ? NSPredicate(format: "value == '' OR value == nil")
            : NSPredicate(format: "value == %@", value)
        let expectation = XCTNSPredicateExpectation(predicate: predicate, object: element)
        return XCTWaiter.wait(for: [expectation], timeout: timeout) == .completed
    }
}

/// Boot, composer and completion checks.
@MainActor
final class RundalePhase2UITests: RundalePhase2UITestCase {
    func testPhase2FoundationBootsCanonicalWorldAndPeig() {
        launch(reset: true)

        let header = app.otherElements["status.header"]
        XCTAssertTrue(header.waitForExistence(timeout: 8))
        XCTAssertTrue(header.label.localizedCaseInsensitiveContains("Kilteevan Village"))
        XCTAssertTrue(waitForTranscriptText(Self.opening, timeout: 8))

        submit("/look")
        XCTAssertTrue(waitForTranscriptItem("It is morning.", timeout: 8))
        // Peig is walking up from the Letter Office at 07:00; she waits on
        // the village road for the morning post.
        submit("/wait 10")
        submit("/people")
        XCTAssertTrue(waitForTranscriptItem("a satchel of letters", timeout: 8))
    }

    func testPhase2LookBatchClearsComposerAfterRustAcceptance() {
        launch(reset: true)
        waitForInitialScene()

        let input = commandInput
        input.tap()
        input.typeText("/look")
        app.buttons["composer.send"].tap()

        XCTAssertTrue(waitForTranscriptItem("It is morning.", timeout: 8))
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

        // The scene /look prints can push the command row out of the lazy
        // transcript on a small screen, so read it from the trace.
        XCTAssertTrue(app.waitForTranscriptRow(timeout: 8) {
            $0.kind == "player_command" && $0.text.contains("/look")
        })
        XCTAssertTrue(waitForValue("", on: input, timeout: 8))
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 3))
#endif
    }

    func testPhase2CompletionsUseRustNearbyPeople() {
        launch(reset: true)
        waitForInitialScene()
        goToTheCottage()

        let input = commandInput
        XCTAssertTrue(input.waitForExistence(timeout: 5))
        input.tap()
        input.typeText("ask @")

        // Mícheál and Róisín are at home; Peig is not here.
        XCTAssertTrue(app.buttons["completion.npc-2"].waitForExistence(timeout: 8))
        XCTAssertTrue(app.buttons["completion.npc-3"].exists)
        XCTAssertFalse(app.buttons["completion.npc-1"].exists)
    }
}

/// Streaming dialogue and failure-category checks.
@MainActor
final class RundalePhase2DialogueUITests: RundalePhase2UITestCase {
    func testPhase2IncrementalChunksUpdateOneDialogueRowBeforeFinal() {
        launch(reset: true)
        waitForInitialScene()
        goToTheCottage()

        submit("ask Mícheál about the cattle")

        let provisional = dialogueRow(containing: "The wet ground")
        XCTAssertTrue(provisional.waitForExistence(timeout: 8))
        let rowID = provisional.identifier
        XCTAssertFalse(rowID.isEmpty)
        XCTAssertTrue(provisional.label.contains("In progress"))

        XCTAssertTrue(waitForDialogue(containing: "has made moving cattle", timeout: 12))
        XCTAssertTrue(waitForDialogue(containing: "difficult this week", timeout: 12))
        let completed = app.descendants(matching: .any)
            .matching(identifier: rowID)
            .firstMatch
        XCTAssertTrue(completed.waitForExistence(timeout: 5))
        XCTAssertFalse(completed.label.contains("In progress"))
    }

    func testPhase2EndpointErrorNamesTheFailureCategory() {
        launch(reset: true)
        waitForInitialScene()
        goToTheCottage()

        submit("ask Mícheál to fail")

        // The mod's line for an unavailable provider, not the generic one.
        XCTAssertTrue(
            waitForTranscriptText(
                "The storyteller has gone out to the bog and isn't back yet. Try again in a while.",
                timeout: 8
            )
        )
        XCTAssertFalse(waitForTranscriptText("could not be completed", timeout: 1))
    }
}

/// Stop, retry and relaunch-restore checks.
@MainActor
final class RundalePhase2RecoveryUITests: RundalePhase2UITestCase {
    func testPhase2StopLeavesInterruptedAttemptAndRetryCompletes() {
        launch(reset: true)
        waitForInitialScene()
        goToTheCottage()

        submit("ask Mícheál about the cattle slowly")
        let stop = app.buttons["composer.stop"]
        XCTAssertTrue(stop.waitForExistence(timeout: 8))
        stop.tap()

        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
        XCTAssertTrue(waitForTranscriptText("Interrupted; not applied", timeout: 8))
        let retry = app.buttons["composer.retry"]
        XCTAssertTrue(retry.waitForExistence(timeout: 8))
        retry.tap()

        XCTAssertTrue(waitForDialogue(containing: "The wet ground has made moving cattle", timeout: 12))
        XCTAssertTrue(waitForDialogue(containing: "difficult this week", timeout: 20))
        XCTAssertTrue(waitForTranscriptText("Interrupted; not applied", timeout: 8))
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
        XCTAssertFalse(app.buttons["composer.retry"].exists)
    }

    func testPhase2SQLiteRestoresCommittedDialogueAfterRelaunch() {
        launch(reset: true)
        waitForInitialScene()
        goToTheCottage()

        let command = "ask Mícheál about the cattle"
        submit(command)
        XCTAssertTrue(waitForDialogue(containing: "difficult this week", timeout: 12))
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))

        app.terminate()
        app = XCUIApplication()
        launch(reset: false)

        XCTAssertTrue(app.otherElements["status.header"].waitForExistence(timeout: 8))
        // The restored command row can sit above the visible rows on a small
        // screen, so read it from the trace.
        XCTAssertTrue(app.waitForTranscriptRow(timeout: 12) {
            $0.kind == "player_command" && $0.text.contains(command)
        })
        XCTAssertTrue(waitForDialogue(containing: "The wet ground has made moving cattle", timeout: 12))
        XCTAssertTrue(waitForDialogue(containing: "difficult this week", timeout: 12))
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
    }
}
