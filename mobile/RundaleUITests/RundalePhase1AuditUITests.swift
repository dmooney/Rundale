import XCTest

/// Focused Phase 1 contract checks for the native controls. These tests use
/// the real deterministic fixture adapter; manual stepping makes the Stop
/// boundary reproducible without a clock or an invented test-only state.
@MainActor
final class RundalePhase1AuditUITests: RundaleUITestCase {
    private var app: XCUIApplication!

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

    func testMultilineKeyboardDismissesAndReopensWithoutCoveringComposer() {
        launch()

        let input = commandInput
        input.tap()
        XCTAssertTrue(keyboard.waitForExistence(timeout: 3))
        input.typeText("look around\ncheck the road\nwait by the gate")
        XCTAssertTrue((input.value as? String)?.contains("check the road") == true)
        assertComposerIsUsableAboveKeyboard()

        dismissKeyboard()
        XCTAssertTrue(input.isHittable)
        XCTAssertTrue(send.isHittable)

        input.tap()
        XCTAssertTrue(keyboard.waitForExistence(timeout: 3))
        assertComposerIsUsableAboveKeyboard()
        XCTAssertEqual(input.value as? String, "look around\ncheck the road\nwait by the gate")
        send.tap()
        XCTAssertTrue(
            transcriptRow(
                exactLabel: "Player command. look around\ncheck the road\nwait by the gate"
            ).waitForExistence(timeout: 3)
        )
    }

    func testComposerTracksNativeEmojiKeyboardHeightChange() {
        launch()
        commandInput.tap()
        commandInput.typeText("draft before keyboard change")
        let originalHeight = keyboard.frame.height
        // iOS 26 draws the emoji key below the keyboard element, not in it.
        let emoji = app.buttons["Emoji"]
        XCTAssertTrue(emoji.exists)
        emoji.tap()
        let resize = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            abs(self.keyboard.frame.height - originalHeight) > 1
        }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [resize], timeout: 3), .completed,
                       "This case must actually exercise a native keyboard-height change")
        assertComposerIsUsableAboveKeyboard()
        XCTAssertEqual(commandInput.value as? String, "draft before keyboard change")
        dismissKeyboard()
        XCTAssertTrue(commandInput.isHittable)
    }

    func testAccessibilitySizeKeepsFocusedComposerUsable() {
        exerciseAccessibleFocusedComposer(forceDark: false)
    }

    func testDarkAccessibilitySizeKeepsFocusedComposerUsable() {
        exerciseAccessibleFocusedComposer(forceDark: true)
    }

    /// The composer grows with a long draft, up to its five-line limit,
    /// instead of being squeezed by the transcript (reported on device: it
    /// stopped at two lines).
    func testComposerGrowsWithALongDraft() {
        launch()
        let input = commandInput
        input.tap()
        XCTAssertTrue(keyboard.waitForExistence(timeout: 3))
        input.typeText("Hello")
        let oneLine = input.frame.height
        input.typeText(" there, I have a much longer thing to say about the cattle, the wet road "
            + "west of the village, the letters waiting at the office, and the weather")
        let grown = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in input.frame.height > oneLine * 2.5 }, object: nil
        )
        XCTAssertEqual(XCTWaiter.wait(for: [grown], timeout: 3), .completed,
                       "The composer should grow past two lines (\(oneLine) -> \(input.frame.height))")
        assertComposerIsUsableAboveKeyboard()
        XCTAssertGreaterThan(input.frame.minY - transcript.frame.minY, 44,
                             "Some transcript stays visible above the grown composer")
        let screenshot = XCTAttachment(screenshot: app.screenshot())
        screenshot.name = "Composer grown with a long draft"
        screenshot.lifetime = .keepAlways
        add(screenshot)
    }

    /// The largest standard text size is not an accessibility size, so the
    /// five-line limit applies. A grown composer and the People strip must
    /// still fit above the keyboard on a small iPhone.
    func testLargestStandardSizeFitsGrownComposerAndPeopleStripAboveKeyboard() {
        app.launchArguments = [
            "--ui-tests", "--no-auto-focus", "--reset-fixture",
            "-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryXXXL",
            "--fixture=standard"
        ]
        app.launch()
        let input = commandInput
        XCTAssertTrue(input.waitForExistence(timeout: 3))
        input.tap()
        XCTAssertTrue(keyboard.waitForExistence(timeout: 3))
        input.typeText("I have a much longer thing to say about the cattle, the wet road west "
            + "of the village, the letters waiting at the office, and the weather ")
        app.buttons["composer.people"].tap()
        let option = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH 'completion.'")
        ).firstMatch
        XCTAssertTrue(option.waitForExistence(timeout: 3), "The People strip is showing")
        XCTAssertTrue(option.isHittable)
        XCTAssertLessThanOrEqual(option.frame.maxY, keyboard.frame.minY + 1)
        assertComposerIsUsableAboveKeyboard()
        XCTAssertGreaterThan(input.frame.height, 60, "The field grew past one line")
        // Prove the row works: a tap that lands on Commands switches the
        // strip to command options.
        app.buttons["composer.commands"].tap()
        XCTAssertTrue(app.buttons["completion.look"].waitForExistence(timeout: 3),
                      "Commands is reachable above the keyboard")
        XCTAssertGreaterThan(input.frame.minY - transcript.frame.minY, 44,
                             "Some transcript stays visible above the strip and composer")
        let screenshot = XCTAttachment(screenshot: app.screenshot())
        screenshot.name = "Grown composer and People strip at xxxLarge"
        screenshot.lifetime = .keepAlways
        add(screenshot)
    }

    func testAccessibilitySizeKeepsCompletionAndClarificationControlsAboveKeyboard() {
        launchAccessibilityFixture()
        commandInput.tap()
        XCTAssertTrue(keyboard.waitForExistence(timeout: 3))
        commandInput.typeText("@")
        let completion = app.buttons["completion.npc-peig"]
        XCTAssertTrue(completion.waitForExistence(timeout: 3))
        assertCompactAccessoryIsUsable(completion)

        app.terminate()
        launchAccessibilityFixture()
        commandInput.tap()
        XCTAssertTrue(keyboard.waitForExistence(timeout: 3))
        commandInput.typeText("ambiguous")
        send.tap()
        let step = app.buttons["fixture.step"]
        XCTAssertTrue(step.waitForExistence(timeout: 3))
        step.tap()
        step.tap()
        let option = app.buttons["clarification.option.micheal"]
        XCTAssertTrue(option.waitForExistence(timeout: 3))
        assertCompactAccessoryIsUsable(option)
        app.buttons["composer.people"].tap()
        let completionWhileClarifying = app.buttons["completion.npc-peig"]
        XCTAssertTrue(completionWhileClarifying.waitForExistence(timeout: 3))
        XCTAssertTrue(option.exists)
        assertCompactAccessoryIsUsable(option)
        assertCompactAccessoryIsUsable(completionWhileClarifying)
    }

    private func exerciseAccessibleFocusedComposer(forceDark: Bool) {
        launchAccessibilityFixture(forceDark: forceDark)

        let input = commandInput
        XCTAssertTrue(input.waitForExistence(timeout: 3))
        XCTAssertTrue(input.label.contains("Command draft"))
        for id in ["composer.people", "composer.commands"] {
            let shortcut = app.buttons[id]
            XCTAssertTrue(shortcut.isHittable)
            XCTAssertTrue(app.frame.contains(shortcut.frame))
            XCTAssertGreaterThanOrEqual(shortcut.frame.width, 44 - 1e-9)
            XCTAssertGreaterThanOrEqual(shortcut.frame.height, 44 - 1e-9)
        }
        input.tap()
        XCTAssertTrue(keyboard.waitForExistence(timeout: 3), "Tapping the composer must establish focus")
        input.typeText("/look")
        XCTAssertEqual(input.value as? String, "/look")
        assertComposerIsUsableAboveKeyboard()
        XCTAssertTrue(send.isEnabled)
        send.tap()
        let hierarchy = XCTAttachment(string: app.debugDescription)
        hierarchy.name = "Accessibility-size state immediately after Send"
        hierarchy.lifetime = .keepAlways
        add(hierarchy)
        XCTAssertTrue(
            transcriptRow(exactLabel: "Player command. /look").waitForExistence(timeout: 3)
        )
        assertUsableTranscript(transcriptRow(exactLabel: "Player command. /look"))
        XCTAssertTrue(keyboard.exists, "Send should return focus to the composer")
        XCTAssertTrue(commandInput.isHittable)
        let stop = app.buttons["composer.stop"]
        XCTAssertTrue(stop.isHittable)
        XCTAssertLessThanOrEqual(stop.frame.maxY, keyboard.frame.minY + 1)
        let step = app.buttons["fixture.step"]
        XCTAssertTrue(step.waitForExistence(timeout: 3))
        XCTAssertLessThanOrEqual(step.frame.maxY, keyboard.frame.minY + 1)
        step.tap()
        step.tap()
        step.tap()
        XCTAssertTrue(send.waitForExistence(timeout: 3))
        XCTAssertTrue(transcriptRow(exactLabel: "Result. Crossroads · stone wall · lane to the east")
            .waitForExistence(timeout: 3))

        commandInput.typeText("long stream")
        XCTAssertEqual(commandInput.value as? String, "long stream")
        send.tap()
        XCTAssertTrue(stop.waitForExistence(timeout: 3))
        XCTAssertTrue(step.waitForExistence(timeout: 3))
        XCTAssertLessThanOrEqual(step.frame.maxY, keyboard.frame.minY + 1)
        step.tap()
        step.tap()
        let reply = transcriptRow(exactLabel: "Dialogue. Peig. The first part arrives.. In progress")
        XCTAssertTrue(reply.waitForExistence(timeout: 3))
        assertUsableTranscript(reply)
        let screenshot = XCTAttachment(screenshot: app.screenshot())
        screenshot.name = "Accessibility-size transcript, composer, and streamed reply"
        screenshot.lifetime = .keepAlways
        add(screenshot)
        stop.tap()
        XCTAssertTrue(send.waitForExistence(timeout: 3))
        assertUsableTranscript(transcriptRow(
            exactLabel: "Dialogue. Peig. The first part arrives.. Interrupted; not applied"
        ))
        commandInput.typeText("next draft")
        XCTAssertEqual(commandInput.value as? String, "next draft")
        XCTAssertTrue(keyboard.exists)
    }

    private func launch() {
        app.launchArguments = [
            "--ui-tests", "--no-auto-focus", "--reset-fixture", "--fixture=standard"
        ]
        app.launch()
        XCTAssertTrue(transcript.waitForExistence(timeout: 3))
        XCTAssertTrue(commandInput.waitForExistence(timeout: 3))
    }

    private func launchAccessibilityFixture(forceDark: Bool = false) {
        app.launchArguments = [
            "--ui-tests", "--no-auto-focus", "--reset-fixture",
            "-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL",
            "--fixture=standard"
        ]
        if forceDark {
            app.launchArguments.append("--force-dark-appearance")
        }
        app.launch()
    }

    private func dismissKeyboard() {
        // Interactive dismissal requires dragging into the keyboard, not a
        // short swipe ending within the transcript above the composer.
        transcript.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
            .press(forDuration: 0.1, thenDragTo:
                keyboard.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.9)))
        XCTAssertTrue(keyboard.waitForNonExistence(timeout: 3))
    }

    private func assertComposerIsUsableAboveKeyboard() {
        XCTAssertTrue(commandInput.isHittable)
        XCTAssertTrue(send.isHittable)
        XCTAssertLessThanOrEqual(commandInput.frame.maxY, keyboard.frame.minY + 1)
        XCTAssertLessThanOrEqual(send.frame.maxY, keyboard.frame.minY + 1)
    }

    private func assertUsableTranscript(_ row: XCUIElement) {
        XCTAssertGreaterThanOrEqual(transcript.frame.height, 132,
                                    "Dynamic Type must leave a usable transcript viewport")
        XCTAssertTrue(transcript.frame.intersects(app.frame))
        XCTAssertTrue(row.isHittable)
        XCTAssertTrue(row.frame.intersects(transcript.frame))
    }

    private func assertCompactAccessoryIsUsable(_ control: XCUIElement) {
        XCTAssertTrue(control.isHittable)
        XCTAssertLessThanOrEqual(control.frame.maxY, keyboard.frame.minY + 1)
        XCTAssertGreaterThanOrEqual(transcript.frame.height, 88)
        XCTAssertTrue(transcript.frame.intersects(app.frame))
    }

    private var commandInput: XCUIElement {
        app.descendants(matching: .any)
            .matching(identifier: "composer.input")
            .firstMatch
    }

    private var send: XCUIElement { app.buttons["composer.send"] }
    private var keyboard: XCUIElement { app.keyboards.firstMatch }

    private var transcript: XCUIElement {
        app.descendants(matching: .any)
            .matching(identifier: "transcript")
            .firstMatch
    }

    private func transcriptRow(exactLabel: String) -> XCUIElement {
        exactTranscriptRows(label: exactLabel).firstMatch
    }

    private func exactTranscriptRows(label: String) -> XCUIElementQuery {
        app.descendants(matching: .any).matching(
            NSPredicate(
                format: "identifier BEGINSWITH %@ AND label == %@",
                "transcript.item.", label
            )
        )
    }
}

private extension XCUIElement {
    func waitForNonExistence(timeout: TimeInterval) -> Bool {
        let expectation = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "exists == false"), object: self
        )
        return XCTWaiter.wait(for: [expectation], timeout: timeout) == .completed
    }
}
