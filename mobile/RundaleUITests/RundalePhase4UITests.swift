import XCTest

/// Production Rust/SQLite and SwiftUI recovery; only the remote transport is
/// fault-injected. These run on simulator or device; human acceptance is separate.
@MainActor
final class RundalePhase4UITests: XCTestCase {
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

    func testDraftAndCommittedTravelSurviveBackgroundAndTermination() {
        launch(reset: true)
        submit("go east")
        XCTAssertTrue(header(containing: "Letter Office").waitForExistence(timeout: 8))
        let draft = "Ask about tomorrow's letters"
        input.tap()
        input.typeText(draft)

        backgroundAndReturn()
        XCTAssertEqual(input.value as? String, draft)
        XCTAssertTrue(header(containing: "Letter Office").exists)
        relaunch()
        XCTAssertEqual(input.value as? String, draft)
        XCTAssertTrue(header(containing: "Letter Office").exists)
        XCTAssertFalse(app.buttons["composer.retry"].exists)
        assertSingleCommand("go east")
        attach("Draft and location restored")
    }

    func testBackgroundInterruptsStreamAndPreservesNewDraftForRetry() {
        launch(reset: true)
        submit("ask Peig about the church slowly")
        XCTAssertTrue(rows(containing: "The rain keeps").firstMatch.waitForExistence(timeout: 10))
        input.tap()
        input.typeText("My next question")
        backgroundAndReturn()

        XCTAssertTrue(rows(containing: "Interrupted; not applied").firstMatch.waitForExistence(timeout: 8))
        XCTAssertEqual(input.value as? String, "My next question")
        XCTAssertFalse(app.buttons["composer.stop"].exists)
        let retry = app.buttons["composer.retry"]
        XCTAssertTrue(retry.waitForExistence(timeout: 5))
        retry.tap()
        waitForCompletedDialogue()
        XCTAssertEqual(input.value as? String, "My next question")
        assertSingleCommand("ask Peig about the church slowly")
        XCTAssertFalse(retry.exists)
        attach("Background interruption recovered")
    }

    func testTerminationDuringStreamingRecoversOneRequestAndRetriesOnce() {
        launch(reset: true)
        let command = "ask Peig about the church slowly"
        submit(command)
        XCTAssertTrue(rows(containing: "The rain keeps").firstMatch.waitForExistence(timeout: 10))
        app.terminate()
        launch(reset: false)
        XCTAssertTrue(app.buttons["composer.retry"].waitForExistence(timeout: 8))
        XCTAssertFalse(app.buttons["composer.stop"].exists)
        assertSingleCommand(command)
        XCTAssertTrue(rows(containing: "Interrupted; not applied").firstMatch.exists)
        app.buttons["composer.retry"].tap()
        waitForCompletedDialogue()
        relaunch()
        assertSingleCommand(command)
        XCTAssertEqual(completedDialogue.count, 1)
        XCTAssertFalse(app.buttons["composer.retry"].exists)
    }

    func testConnectionLossBeforeResponseCanRetryWithoutRestart() {
        assertNetworkRecovery(command: "ask Peig offline once", partialExpected: false)
    }

    func testConnectionLossDuringStreamCanRetryWithoutDuplicatingDialogue() {
        assertNetworkRecovery(command: "ask Peig disconnect once", partialExpected: true)
    }

    func testCompletedActionStaysCompletedThroughRepeatedAppSwitching() {
        launch(reset: true)
        submit("ask Peig about the church")
        waitForCompletedDialogue()
        backgroundAndReturn()
        backgroundAndReturn()
        XCTAssertEqual(completedDialogue.count, 1)
        assertSingleCommand("ask Peig about the church")
        XCTAssertFalse(app.buttons["composer.retry"].exists)
        relaunch()
        XCTAssertEqual(completedDialogue.count, 1)
        XCTAssertFalse(app.buttons["composer.retry"].exists)
    }

    func testTalkingAboutAbsentMichaelStillGetsPeigsReplyAtTheLetterOffice() {
        launch(reset: true)
        // Use the canonical morning route so Peig has reached the office;
        // on the first turn she is still travelling from the village.
        submit("go west")
        submit("go east")
        submit("go east")
        XCTAssertTrue(header(containing: "Letter Office").waitForExistence(timeout: 8))
        submit("Hello")
        waitForCompletedDialogue()
        let firstReplyID = completedDialogue.firstMatch.identifier
        let command = "Well I’m looking for work and a place to stay. Michael said maybe you could direct me."
        submit(command)
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 25))
        let reply = rows(containing: "Peig Hannigan").matching(NSPredicate(
            format: "identifier != %@ AND label CONTAINS 'Dialogue' AND NOT label CONTAINS 'In progress'",
            firstReplyID
        )).firstMatch
        XCTAssertTrue(reply.waitForExistence(timeout: 5))
        XCTAssertTrue(reply.label.contains("Peig Hannigan"))
        let scroll = app.collectionViews["transcript"]
        XCTAssertLessThanOrEqual(reply.frame.maxY, scroll.frame.maxY + 2,
                                 "The full new reply should be visible above the keyboard")
        XCTAssertFalse(app.buttons["transcript.new-text"].exists)
        XCTAssertFalse(rows(containing: "is not here").firstMatch.exists)
        attach("Mentioning absent Michael while speaking to Peig")
        assertSingleCommand(command)
    }

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
            input.typeText("go east")
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
        }
    }

    private func assertNetworkRecovery(command: String, partialExpected: Bool) {
        launch(reset: true)
        submit(command)
        let retry = app.buttons["composer.retry"]
        XCTAssertTrue(retry.waitForExistence(timeout: 12))
        XCTAssertEqual(completedDialogue.count, 0)
        if partialExpected {
            XCTAssertTrue(rows(containing: "The rain keeps").firstMatch.exists)
        }
        XCTAssertTrue(rows(containing: "retry").firstMatch.exists)
        XCTAssertFalse(app.buttons["composer.stop"].exists)
        retry.tap()
        waitForCompletedDialogue()
        assertSingleCommand(command)
        XCTAssertEqual(completedDialogue.count, 1)
        XCTAssertFalse(retry.exists)
        relaunch()
        XCTAssertEqual(completedDialogue.count, 1)
        assertSingleCommand(command)
        attach("Connection recovered without duplicate action")
    }

    private func launch(reset: Bool, extra: [String] = []) {
        app.launchArguments = ["--ui-tests", "--phase3", "--phase3-mock", "--no-auto-focus"] + extra
        if reset { app.launchArguments.append("--reset-fixture") }
        app.launch()
        XCTAssertTrue(app.otherElements["status.header"].waitForExistence(timeout: 10))
        XCTAssertTrue(input.waitForExistence(timeout: 5))
        if reset {
            XCTAssertTrue(rows(containing: "Morning gathers").firstMatch.waitForExistence(timeout: 8))
        }
    }

    private func relaunch() {
        app.terminate()
        launch(reset: false)
    }

    private func backgroundAndReturn() {
        XCUIDevice.shared.press(.home)
        XCTAssertTrue(app.wait(for: .runningBackground, timeout: 5))
        app.activate()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 5))
    }

    private func submit(_ command: String) {
        input.tap()
        input.typeText(command)
        app.buttons["composer.send"].tap()
        let cleared = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == '' OR value == nil"), object: input)
        XCTAssertEqual(XCTWaiter.wait(for: [cleared], timeout: 8), .completed)
    }

    /// UICollectionView exposes instantiated rows, not the whole journal. Walk
    /// this short session to its opening scene and count stable command IDs.
    private func assertSingleCommand(_ command: String, file: StaticString = #filePath, line: UInt = #line) {
        let scroll = app.collectionViews["transcript"]
        var commandIDs = Set<String>()
        var swipes = 0
        var reachedOpening = false
        for _ in 0..<10 {
            for row in rows(containing: command).allElementsBoundByIndex {
                commandIDs.insert(row.identifier)
            }
            if rows(containing: "Morning gathers").firstMatch.isHittable {
                reachedOpening = true
                break
            }
            scroll.swipeDown()
            swipes += 1
        }
        XCTAssertTrue(reachedOpening, "Inspect the entire short session before counting commands", file: file, line: line)
        XCTAssertEqual(commandIDs.count, 1, "Exactly one logical command must survive recovery", file: file, line: line)
        // Restore the tail so later assertions inspect the completion and so
        // subsequent input keeps the same newest-following behavior.
        for _ in 0...swipes { scroll.swipeUp() }
        let newest = app.buttons["transcript.new-text"]
        if newest.exists { newest.tap() }
    }

    private func waitForCompletedDialogue() {
        XCTAssertTrue(completedDialogue.firstMatch.waitForExistence(timeout: 25))
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 5))
    }

    private var input: XCUIElement {
        app.descendants(matching: .any).matching(identifier: "composer.input").firstMatch
    }

    private func header(containing text: String) -> XCUIElement {
        app.otherElements.matching(NSPredicate(
            format: "identifier == 'status.header' AND label CONTAINS %@", text
        )).firstMatch
    }

    private func rows(containing text: String) -> XCUIElementQuery {
        app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS %@", text
        ))
    }

    private var completedDialogue: XCUIElementQuery {
        app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS 'Dialogue' AND label CONTAINS 'stands beyond the alder trees' AND NOT label CONTAINS 'In progress' AND NOT label CONTAINS 'Interrupted'"
        ))
    }

    private func attach(_ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
