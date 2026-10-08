import XCTest

/// Production Rust/SQLite and SwiftUI recovery; only the remote transport is
/// fault-injected. These run on simulator or device; human acceptance is separate.
@MainActor
class RundalePhase4UITestCase: XCTestCase {
    var app: XCUIApplication!

    override func setUp() async throws {
        try await super.setUp()
        continueAfterFailure = false
        app = XCUIApplication()
    }

    override func tearDown() async throws {
        app?.terminate()
        app = nil
        try await super.tearDown()
    }

    func assertNetworkRecovery(command: String, partialExpected: Bool) {
        launch(reset: true)
        hold(3)
        goToTheCottage()
        hold(3)
        submit(command)
        let retry = app.buttons["composer.retry"]
        XCTAssertTrue(retry.waitForExistence(timeout: 12))
        XCTAssertEqual(completedDialogue.count, 0)
        if partialExpected {
            XCTAssertTrue(rows(containing: "The wet ground").firstMatch.exists)
            XCTAssertTrue(rows(containing: "The wet ground").firstMatch.label.contains("not applied"))
            attach("Partial reply marked not applied")
        }
        XCTAssertTrue(rows(containing: "The road out of the parish is washed away").firstMatch.exists)
        XCTAssertFalse(app.buttons["composer.stop"].exists)
        hold(6)
        retry.tap()
        waitForCompletedDialogue()
        hold(4)
        assertSingleCommand(command)
        XCTAssertEqual(completedDialogue.count, 1)
        XCTAssertFalse(retry.exists)
        relaunch()
        XCTAssertEqual(completedDialogue.count, 1)
        assertSingleCommand(command)
        attach("Connection recovered without duplicate action")
        hold(5)
    }

    func launch(reset: Bool, extra: [String] = []) {
        app.launchArguments = ["--ui-tests", "--phase3", "--phase3-mock", "--no-auto-focus"] + extra
        if reset { app.launchArguments.append("--reset-fixture") }
        app.launch()
        XCTAssertTrue(app.otherElements["status.header"].waitForExistence(timeout: 10))
        XCTAssertTrue(input.waitForExistence(timeout: 5))
        if reset {
            XCTAssertTrue(rows(containing: Self.opening).firstMatch.waitForExistence(timeout: 8))
        }
    }

    /// The opening scene on the canonical world (`mods/rundale`).
    static let opening = "A muddy road runs between low stone walls"

    /// Mícheál and Róisín are at home in the morning. Movement is parsed
    /// locally, so no Endpoint is called.
    func goToTheCottage() {
        submit("go to Connolly Cottage")
        XCTAssertTrue(header(containing: "Connolly Cottage").waitForExistence(timeout: 8))
    }

    func relaunch() {
        app.terminate()
        launch(reset: false)
    }

    func backgroundAndReturn() {
        XCUIDevice.shared.press(.home)
        XCTAssertTrue(app.wait(for: .runningBackground, timeout: 5))
        app.activate()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 5))
    }

    func submit(_ command: String) {
        input.tap()
        input.typeText(command)
        app.buttons["composer.send"].tap()
        let cleared = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == '' OR value == nil"), object: input)
        XCTAssertEqual(XCTWaiter.wait(for: [cleared], timeout: 8), .completed)
    }

    /// UICollectionView exposes only instantiated rows, and on a small screen
    /// a long scene pushes earlier rows out of it. The UI-test trace holds the
    /// whole published transcript, including rows restored after relaunch.
    func assertSingleCommand(_ command: String, file: StaticString = #filePath, line: UInt = #line) {
        let rows = app.transcriptRows()
        XCTAssertTrue(rows.contains { $0.text.contains(Self.opening) },
                      "Inspect the entire short session before counting commands: \(rows)",
                      file: file, line: line)
        let commands = app.playerCommandRows(containing: command)
        XCTAssertEqual(commands.count, 1, "Exactly one logical command must survive recovery: \(commands)",
                       file: file, line: line)
    }

    func waitForCompletedDialogue() {
        XCTAssertTrue(completedDialogue.firstMatch.waitForExistence(timeout: 25))
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 5))
    }

    var input: XCUIElement {
        app.descendants(matching: .any).matching(identifier: "composer.input").firstMatch
    }

    func header(containing text: String) -> XCUIElement {
        app.otherElements.matching(NSPredicate(
            format: "identifier == 'status.header' AND label CONTAINS %@", text
        )).firstMatch
    }

    func rows(containing text: String) -> XCUIElementQuery {
        app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS %@", text
        ))
    }

    var completedDialogue: XCUIElementQuery {
        app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS 'Dialogue' AND (label CONTAINS 'difficult this week' OR label CONTAINS 'stands beyond the alder trees') AND NOT label CONTAINS 'In progress' AND NOT label CONTAINS 'Interrupted'"
        ))
    }

    func attach(_ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}

/// Draft, repeated app switching and absent-name replies.
@MainActor
final class RundalePhase4UITests: RundalePhase4UITestCase {
    func testDraftAndCommittedTravelSurviveBackgroundAndTermination() {
        launch(reset: true)
        hold(3)
        submit("go to the Letter Office")
        XCTAssertTrue(header(containing: "Letter Office").waitForExistence(timeout: 8))
        hold(3)
        let draft = "Ask about tomorrow's letters"
        input.tap()
        input.typeText(draft)
        hold(2)

        backgroundAndReturn()
        XCTAssertEqual(input.value as? String, draft)
        XCTAssertTrue(header(containing: "Letter Office").exists)
        hold(3)
        relaunch()
        hold(4)
        XCTAssertEqual(input.value as? String, draft)
        XCTAssertTrue(header(containing: "Letter Office").exists)
        XCTAssertFalse(app.buttons["composer.retry"].exists)
        assertSingleCommand("go to the Letter Office")
        attach("Draft and location restored")
    }

    /// The app supports portrait only (#2211): turning the phone with a draft
    /// and the keyboard up leaves the layout, the draft and the controls alone.
    func testTurningThePhoneKeepsThePortraitComposerAndDraft() {
        launch(reset: true)
        addTeardownBlock { @MainActor in XCUIDevice.shared.orientation = .portrait }
        let draft = "go to the Letter"
        input.tap()
        input.typeText(draft)
        let portraitTranscript = app.collectionViews["transcript"].frame
        hold(3)

        for orientation: UIDeviceOrientation in [.landscapeLeft, .landscapeRight] {
            XCUIDevice.shared.orientation = orientation
            hold(3)
            XCTAssertGreaterThan(app.frame.height, app.frame.width,
                                 "The interface must stay portrait when the phone turns")
            XCTAssertEqual(app.collectionViews["transcript"].frame, portraitTranscript)
            XCTAssertEqual(input.value as? String, draft)
            for identifier in ["composer.send", "composer.people", "composer.commands"] {
                XCTAssertTrue(app.buttons[identifier].isHittable, "\(identifier) after turning to \(orientation)")
            }
        }
        attach("Portrait composer with the phone turned")

        input.typeText(" Office")
        app.buttons["composer.send"].tap()
        XCTAssertTrue(header(containing: "Letter Office").waitForExistence(timeout: 8))
        hold(3)
        XCUIDevice.shared.orientation = .portrait
        XCTAssertEqual(input.value as? String ?? "", "")
        assertSingleCommand("go to the Letter Office")
    }

    func testCompletedActionStaysCompletedThroughRepeatedAppSwitching() {
        launch(reset: true)
        goToTheCottage()
        submit("ask Mícheál about the cattle")
        waitForCompletedDialogue()
        backgroundAndReturn()
        backgroundAndReturn()
        XCTAssertEqual(completedDialogue.count, 1)
        assertSingleCommand("ask Mícheál about the cattle")
        XCTAssertFalse(app.buttons["composer.retry"].exists)
        relaunch()
        XCTAssertEqual(completedDialogue.count, 1)
        XCTAssertFalse(app.buttons["composer.retry"].exists)
    }

    func testTalkingAboutAbsentMichaelStillGetsPeigsReplyAtTheLetterOffice() {
        launch(reset: true)
        // Peig's schedule has her set out for the Letter Office at 09:00. A
        // long wait moves the clock in one step, so she only sets off when it
        // ends; a short one then lets her arrive (as the canonical world
        // sheet's script does).
        submit("/wait 120")
        submit("/wait 10")
        submit("go to the Letter Office")
        XCTAssertTrue(header(containing: "Letter Office").waitForExistence(timeout: 8))
        submit("Hello")
        waitForCompletedDialogue()
        let firstReplyID = completedDialogue.firstMatch.identifier
        let command = "Well I’m looking for work and a place to stay. Michael said maybe you could direct me."
        submit(command)
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 25))
        let reply = rows(containing: "A sharp-eyed woman with a satchel of letters").matching(NSPredicate(
            format: "identifier != %@ AND label CONTAINS 'Dialogue' AND NOT label CONTAINS 'In progress'",
            firstReplyID
        )).firstMatch
        XCTAssertTrue(reply.waitForExistence(timeout: 5))
        XCTAssertTrue(reply.label.contains("A sharp-eyed woman with a satchel of letters"))
        let scroll = app.collectionViews["transcript"]
        XCTAssertLessThanOrEqual(reply.frame.maxY, scroll.frame.maxY + 2,
                                 "The full new reply should be visible above the keyboard")
        XCTAssertFalse(app.buttons["transcript.new-text"].exists)
        XCTAssertFalse(rows(containing: "is not here").firstMatch.exists)
        attach("Mentioning absent Michael while speaking to Peig")
        assertSingleCommand(command)
    }
}

/// Background and termination during streaming.
@MainActor
final class RundalePhase4RecoveryUITests: RundalePhase4UITestCase {
    func testBackgroundInterruptsStreamAndPreservesNewDraftForRetry() {
        launch(reset: true)
        goToTheCottage()
        submit("ask Mícheál about the cattle slowly")
        XCTAssertTrue(rows(containing: "The wet ground").firstMatch.waitForExistence(timeout: 10))
        input.tap()
        input.typeText("My next question")
        backgroundAndReturn()

        XCTAssertTrue(rows(containing: "Interrupted; not applied").firstMatch.waitForExistence(timeout: 8))
        hold(5)
        XCTAssertEqual(input.value as? String, "My next question")
        XCTAssertFalse(app.buttons["composer.stop"].exists)
        let retry = app.buttons["composer.retry"]
        XCTAssertTrue(retry.waitForExistence(timeout: 5))
        retry.tap()
        waitForCompletedDialogue()
        hold(5)
        XCTAssertEqual(input.value as? String, "My next question")
        assertSingleCommand("ask Mícheál about the cattle slowly")
        XCTAssertFalse(retry.exists)
        attach("Background interruption recovered")
    }

    func testTerminationDuringStreamingRecoversOneRequestAndRetriesOnce() {
        launch(reset: true)
        goToTheCottage()
        let command = "ask Mícheál about the cattle slowly"
        submit(command)
        XCTAssertTrue(rows(containing: "The wet ground").firstMatch.waitForExistence(timeout: 10))
        app.terminate()
        launch(reset: false)
        XCTAssertTrue(app.buttons["composer.retry"].waitForExistence(timeout: 8))
        XCTAssertFalse(app.buttons["composer.stop"].exists)
        assertSingleCommand(command)
        // Streamed text is provisional and not journaled, so a killed process
        // leaves the interruption notice, not the partial reply.
        XCTAssertTrue(rows(containing: "The previous response was interrupted").firstMatch.exists)
        app.buttons["composer.retry"].tap()
        waitForCompletedDialogue()
        relaunch()
        assertSingleCommand(command)
        XCTAssertEqual(completedDialogue.count, 1)
        XCTAssertFalse(app.buttons["composer.retry"].exists)
    }

    /// Killed the moment Send is tapped: the command is kept once and can be
    /// retried, and no activity indicator or Stop survives the relaunch.
    func testForceQuitAtSendKeepsOneCommandAndOffersRetry() {
        launch(reset: true)
        goToTheCottage()
        let command = "ask Mícheál about the cattle slowly"
        input.tap()
        input.typeText(command)
        app.buttons["composer.send"].tap()
        app.terminate()
        launch(reset: false)
        XCTAssertTrue(app.buttons["composer.retry"].waitForExistence(timeout: 8))
        hold(5)
        XCTAssertFalse(app.descendants(matching: .any)["composer.waiting"].exists)
        XCTAssertFalse(app.buttons["composer.stop"].exists)
        assertSingleCommand(command)
        XCTAssertEqual(completedDialogue.count, 0)
        app.buttons["composer.retry"].tap()
        waitForCompletedDialogue()
        hold(4)
        relaunch()
        hold(5)
        assertSingleCommand(command)
        XCTAssertEqual(completedDialogue.count, 1)
        XCTAssertFalse(app.buttons["composer.retry"].exists)
    }

    /// Killed just after the reply commits: it is kept and not offered again.
    func testForceQuitAfterTheReplyCommitsKeepsItWithoutRetry() {
        launch(reset: true)
        goToTheCottage()
        let command = "ask Mícheál about the cattle slowly"
        submit(command)
        XCTAssertTrue(app.waitForTranscriptRow(timeout: 15) {
            $0.kind == "npc_dialogue" && $0.state == "committed"
        })
        app.terminate()
        launch(reset: false)
        assertSingleCommand(command)
        XCTAssertEqual(app.committedDialogueRows().count, 1)
        XCTAssertFalse(app.buttons["composer.retry"].exists)
    }
}

/// Connection loss recovery.
@MainActor
final class RundalePhase4NetworkUITests: RundalePhase4UITestCase {
    func testConnectionLossBeforeResponseCanRetryWithoutRestart() {
        assertNetworkRecovery(command: "ask Mícheál offline once", partialExpected: false)
    }

    func testConnectionLossDuringStreamCanRetryWithoutDuplicatingDialogue() {
        assertNetworkRecovery(command: "ask Mícheál disconnect once", partialExpected: true)
    }
}

/// Accessibility sizes and dark appearance.
@MainActor
final class RundalePhase4AccessibilityUITests: RundalePhase4UITestCase {
    func testNativeWorldCoreLoopAtEveryAccessibilitySizeAndDarkAppearance() {
        let sizes = ["AccessibilityM", "AccessibilityL", "AccessibilityXL",
                     "AccessibilityXXL", "AccessibilityXXXL"]
        for size in sizes {
            app.terminate()
            app = XCUIApplication()
            launch(reset: true, extra: ["--force-dark-appearance", "-UIPreferredContentSizeCategoryName",
                                        "UICTContentSizeCategory\(size)"])
            XCTAssertTrue(input.label.contains("Command draft"))
            XCTAssertTrue(app.buttons["composer.send"].label.contains("Send command"))
            XCTAssertTrue(app.buttons["composer.send"].isHittable)
            input.tap()
            input.typeText("go to the Letter Office")
            app.buttons["composer.send"].tap()
            // At accessibility sizes the command may scroll out of the native
            // collection view. The authoritative destination proves execution.
            attach("Accessibility travel result")
            XCTAssertTrue(header(containing: "Letter Office").waitForExistence(timeout: 8))
            XCTAssertEqual(input.value as? String ?? "", "")
            XCTAssertTrue(input.isHittable)
            XCTAssertTrue(app.buttons["composer.send"].isHittable)
            XCTAssertTrue(app.frame.contains(app.buttons["composer.send"].frame),
                          "Send must remain fully inside the screen at accessibility sizes")
            XCTAssertTrue(app.frame.contains(input.frame))
            for identifier in ["composer.people", "composer.commands"] {
                let button = app.buttons[identifier]
                XCTAssertTrue(button.isHittable)
                XCTAssertTrue(app.frame.contains(button.frame))
                // Accessibility-frame subtraction can report 44 points as
                // 43.99999999999994. Tolerate arithmetic noise, not subpixel undersizing.
                let minimumHitDimension: CGFloat = 44 - 1e-9
                XCTAssertGreaterThanOrEqual(button.frame.width, minimumHitDimension)
                XCTAssertGreaterThanOrEqual(button.frame.height, minimumHitDimension)
            }
            attach("Native world at accessibility size \(size)")
            hold(3)
        }
    }
}
