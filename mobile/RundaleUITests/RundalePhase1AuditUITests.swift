import XCTest

/// Focused Phase 1 contract checks for the native controls. These tests use
/// the real deterministic fixture adapter; manual stepping makes the Stop
/// boundary reproducible without a clock or an invented test-only state.
@MainActor
final class RundalePhase1AuditUITests: XCTestCase {
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

    func testAcceptedCommandRowIsExactAndImmediatelyVisible() {
        launch()

        let command = "look around"
        submit(command)

        let row = transcriptRow(exactLabel: "Player command. \(command)")
        XCTAssertTrue(row.waitForExistence(timeout: 1))
        XCTAssertTrue(row.isHittable)
        XCTAssertTrue(row.frame.intersects(transcript.frame))
        XCTAssertEqual(commandInput.value as? String, "")
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
        let emoji = keyboard.buttons["Emoji"]
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

    func testStopRetainsExactPartialRowAndRemovesManualAdvance() {
        launch()

        submit("long stream")
        let step = app.buttons["fixture.step"]
        XCTAssertTrue(step.waitForExistence(timeout: 3))
        XCTAssertLessThanOrEqual(step.frame.maxY, keyboard.frame.minY + 1)

        // First advance is interpretation; second is the first provisional
        // dialogue chunk. The fixture does not advance except through this
        // production-visible control in UI-test mode.
        step.tap()
        step.tap()
        let partial = "Dialogue. Peig. The first part arrives.. In progress"
        let partialRow = transcriptRow(exactLabel: partial)
        XCTAssertTrue(partialRow.waitForExistence(timeout: 3))
        let partialID = partialRow.identifier

        app.buttons["composer.stop"].tap()
        XCTAssertTrue(send.waitForExistence(timeout: 3))

        let retained = app.descendants(matching: .any)
            .matching(identifier: partialID)
            .firstMatch
        XCTAssertTrue(retained.waitForExistence(timeout: 3))
        XCTAssertEqual(
            retained.label,
            "Dialogue. Peig. The first part arrives.. Interrupted; not applied"
        )
        XCTAssertFalse(step.exists, "Stop must remove the only manual stream advance")
        XCTAssertFalse(transcriptRows(containing: "Then the road opens into rain and light.").firstMatch.exists)
        XCTAssertFalse(transcriptRows(containing: "At last, the whole thought is clear.").firstMatch.exists)
    }

    func testRecallPreservesOriginalExactRowAndCompletionRendersExactResult() {
        launch()

        let original = "look around"
        submit(original)
        finishManualStream()
        let originalRow = transcriptRow(exactLabel: "Player command. \(original)")
        XCTAssertTrue(originalRow.waitForExistence(timeout: 3))
        XCTAssertTrue(reveal(originalRow))
        let originalID = originalRow.identifier

        originalRow.tap()
        XCTAssertEqual(commandInput.value as? String, original)
        commandInput.typeText(" again")
        app.buttons["composer.send"].tap()
        let revised = "look around again"
        let revisedRow = transcriptRow(exactLabel: "Player command. \(revised)")
        XCTAssertTrue(revisedRow.waitForExistence(timeout: 3))
        XCTAssertNotEqual(revisedRow.identifier, originalID)
        let retainedOriginal = app.descendants(matching: .any)
            .matching(identifier: originalID)
            .firstMatch
        XCTAssertTrue(retainedOriginal.exists)
        XCTAssertEqual(retainedOriginal.label, "Player command. \(original)")
        XCTAssertEqual(revisedRow.label, "Player command. \(revised)")
        finishManualStream()

        app.buttons["composer.commands"].tap()
        let lookCompletion = app.buttons["completion.look"]
        XCTAssertTrue(lookCompletion.waitForExistence(timeout: 3))
        lookCompletion.tap()
        XCTAssertEqual(commandInput.value as? String, "/look")
        app.buttons["composer.send"].tap()
        finishManualStream()

        let result = "Result. Crossroads · stone wall · lane to the east"
        XCTAssertTrue(transcriptRow(exactLabel: result).waitForExistence(timeout: 3))
    }

    func testTranscriptKindsAndFocusLoopUseProductionControls() {
        launch()
        for prefix in ["Scene. ", "Narration. "] {
            XCTAssertTrue(app.descendants(matching: .any).matching(NSPredicate(
                format: "identifier BEGINSWITH 'transcript.item.' AND label BEGINSWITH %@", prefix
            )).firstMatch.waitForExistence(timeout: 3))
        }
        submit("long stream")
        XCTAssertTrue(transcriptRow(exactLabel: "Player command. long stream").exists)
        let step = app.buttons["fixture.step"]
        step.tap()
        step.tap()
        XCTAssertTrue(transcriptRow(exactLabel: "Dialogue. Peig. The first part arrives.. In progress")
            .waitForExistence(timeout: 3))
        app.buttons["composer.stop"].tap()
        XCTAssertTrue(send.waitForExistence(timeout: 3))
        commandInput.typeText("after stop")
        XCTAssertEqual(commandInput.value as? String, "after stop")
        dismissKeyboard()
        commandInput.tap()
        XCTAssertEqual(commandInput.value as? String, "after stop")
        commandInput.typeText(" and return")
        // A native tap chooses a caret position; it need not choose the end.
        // Verify that focused typing inserts exactly once without losing draft.
        let edited = commandInput.value as? String ?? ""
        XCTAssertEqual(edited.count, "after stop and return".count)
        XCTAssertEqual(edited.replacingOccurrences(of: " and return", with: ""), "after stop")
        app.buttons["composer.commands"].tap()
        app.buttons["completion.look"].tap()
        XCTAssertEqual(commandInput.value as? String, "/look")
        send.tap()
        finishManualStream()
        XCTAssertTrue(transcriptRow(exactLabel: "Result. Crossroads · stone wall · lane to the east")
            .waitForExistence(timeout: 3))
    }

    func testPagedHistoryRestoresOlderAnchorAndReturnsToNewestCompletedReply() {
        launchPagedHistory(reset: true)
        let newest = transcriptRow(exactLabel: "Narration. Historical fixture entry 540.")
        XCTAssertTrue(newest.waitForExistence(timeout: 8))

        commandInput.tap()
        commandInput.typeText("long stream")
        send.tap()
        let step = app.buttons["fixture.step"]
        XCTAssertTrue(step.waitForExistence(timeout: 3))
        step.tap()
        step.tap()
        XCTAssertTrue(transcriptRow(exactLabel: "Dialogue. Peig. The first part arrives.. In progress")
            .waitForExistence(timeout: 3))
        dismissKeyboard()

        let oldest = transcriptRow(exactLabel: "Narration. Historical fixture entry 1.")
        for _ in 0..<120 {
            if oldest.exists && oldest.isHittable { break }
            transcript.swipeDown(velocity: .fast)
        }
        XCTAssertTrue(oldest.exists && oldest.isHittable,
                      "Native gestures must page from the 500-row live window to archived history")
        let anchorY = oldest.frame.minY

        for _ in 0..<3 { step.tap() }
        XCTAssertTrue(send.waitForExistence(timeout: 3))
        XCTAssertTrue(oldest.exists && oldest.isHittable)

        app.terminate()
        launchPagedHistory(reset: false)
        XCTAssertTrue(oldest.waitForExistence(timeout: 8))
        XCTAssertTrue(oldest.isHittable)
        XCTAssertEqual(oldest.frame.minY, anchorY, accuracy: 8,
                       "Relaunch must restore the reader's older-history anchor")

        let newText = app.buttons["transcript.new-text"]
        XCTAssertTrue(newText.waitForExistence(timeout: 3))
        XCTAssertTrue(oldest.exists && oldest.isHittable)
        newText.tap()
        XCTAssertTrue(newText.waitForNonExistence(timeout: 3))
        let completed = transcriptRow(
            exactLabel: "Dialogue. Peig. The first part arrives. Then the road opens into rain and light. At last, the whole thought is clear."
        )
        XCTAssertTrue(completed.waitForExistence(timeout: 3))
        XCTAssertTrue(completed.isHittable)
        XCTAssertTrue(completed.frame.intersects(transcript.frame))
    }

    func testAccessibilitySizeKeepsFocusedComposerUsable() {
        exerciseAccessibleFocusedComposer(forceDark: false)
    }

    func testDarkAccessibilitySizeKeepsFocusedComposerUsable() {
        exerciseAccessibleFocusedComposer(forceDark: true)
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

    private func launchPagedHistory(reset: Bool) {
        app.launchArguments = ["--ui-tests", "--no-auto-focus", "--fixture=paged-history"]
        if reset { app.launchArguments.append("--reset-fixture") }
        app.launch()
        XCTAssertTrue(transcript.waitForExistence(timeout: 5))
    }

    private func submit(_ command: String) {
        commandInput.tap()
        commandInput.typeText(command)
        send.tap()
    }

    private func finishManualStream() {
        let step = app.buttons["fixture.step"]
        for _ in 0..<12 {
            guard step.exists else { return }
            step.tap()
        }
        XCTFail("Fixture did not reach a terminal state")
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

    private func reveal(_ row: XCUIElement) -> Bool {
        for _ in 0..<4 {
            if row.exists, row.isHittable, row.frame.intersects(transcript.frame) {
                return true
            }
            transcript.swipeDown()
        }
        return row.exists && row.isHittable && row.frame.intersects(transcript.frame)
    }

    private func exactTranscriptRows(label: String) -> XCUIElementQuery {
        app.descendants(matching: .any).matching(
            NSPredicate(
                format: "identifier BEGINSWITH %@ AND label == %@",
                "transcript.item.", label
            )
        )
    }

    private func transcriptRows(containing text: String) -> XCUIElementQuery {
        app.descendants(matching: .any).matching(
            NSPredicate(
                format: "identifier BEGINSWITH %@ AND label CONTAINS %@",
                "transcript.item.", text
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
