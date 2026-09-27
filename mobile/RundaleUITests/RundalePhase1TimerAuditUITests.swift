import XCTest

/// Audit-only coverage of the real fixture scheduler. The default cases use
/// the ordinary simulator field; the combined route also proves automatic
/// delivery with the same multiline composer used on an iPhone.
final class RundalePhase1TimerAuditUITests: XCTestCase {
    private var app: XCUIApplication!
    private let first = "The first part arrives."
    private let second = "The first part arrives. Then the road opens into rain and light."
    private let final = "The first part arrives. Then the road opens into rain and light. At last, the whole thought is clear."

    override func setUp() {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments = ["--fixture=long-history", "--reset-fixture", "--no-auto-focus"]
        app.launch()
    }

    private var input: XCUIElement { app.descendants(matching: .any).matching(identifier: "composer.input").firstMatch }
    private var scroll: XCUIElement { app.descendants(matching: .any).matching(identifier: "transcript").firstMatch }
    private func row(_ label: String) -> XCUIElement {
        app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'transcript.item.' AND label == %@", label
        )).firstMatch
    }
    private func wait(_ predicate: @escaping () -> Bool, timeout: TimeInterval = 6) -> Bool {
        let expectation = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in predicate() }, object: nil)
        return XCTWaiter.wait(for: [expectation], timeout: timeout) == .completed
    }
    private func submitStream() {
        XCTAssertTrue(input.waitForExistence(timeout: 5))
        input.tap()
        input.typeText("long stream")
        XCTAssertEqual(input.value as? String, "long stream")
        app.buttons["composer.send"].tap()
        XCTAssertTrue(app.buttons["composer.stop"].waitForExistence(timeout: 3))
        XCTAssertFalse(app.buttons["fixture.step"].exists)
    }
    private func assertVisible(_ element: XCUIElement) {
        XCTAssertTrue(wait { element.exists && element.frame.minY >= self.scroll.frame.minY - 2
            && element.frame.maxY <= self.scroll.frame.maxY + 2 })
        XCTAssertTrue(element.isHittable)
    }

    func testAutomaticChunksStayVisibleAndCompleteWithoutNext() {
        submitStream()
        let start = Date()
        let firstRow = row("Dialogue. Peig. \(first). In progress")
        XCTAssertTrue(firstRow.waitForExistence(timeout: 8))
        let identity = firstRow.identifier
        let firstTime = Date()
        assertVisible(firstRow)
        let secondRow = row("Dialogue. Peig. \(second). In progress")
        XCTAssertTrue(secondRow.waitForExistence(timeout: 5))
        XCTAssertEqual(secondRow.identifier, identity)
        let secondTime = Date()
        assertVisible(secondRow)
        let finalRow = row("Dialogue. Peig. \(final). In progress")
        XCTAssertTrue(finalRow.waitForExistence(timeout: 5))
        XCTAssertEqual(finalRow.identifier, identity)
        assertVisible(finalRow)
        XCTAssertGreaterThan(secondTime.timeIntervalSince(firstTime), 1)
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["composer.stop"].exists)
        XCTAssertFalse(app.buttons["transcript.new-text"].exists)
        let receipt = XCTAttachment(string: "Automatic stream completion: \(Date().timeIntervalSince(start))s; successive exact chunks share \(identity)")
        receipt.lifetime = .keepAlways
        add(receipt)
    }

    func testAutomaticChunksWorkWithNativeMultilineComposer() {
        app.terminate()
        app.launchArguments = [
            "--fixture=standard", "--multiline-simulator-composer",
            "--reset-fixture", "--no-auto-focus"
        ]
        app.launch()

        XCTAssertTrue(input.waitForExistence(timeout: 5))
        input.tap()
        let command = "long stream"
        input.typeText(command)
        XCTAssertEqual(input.value as? String, command)
        app.buttons["composer.send"].tap()
        XCTAssertTrue(app.buttons["composer.stop"].waitForExistence(timeout: 3))
        XCTAssertFalse(app.buttons["fixture.step"].exists)

        let firstRow = row("Dialogue. Peig. \(first). In progress")
        XCTAssertTrue(firstRow.waitForExistence(timeout: 8))
        assertVisible(firstRow)
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
        let completed = row("Dialogue. Peig. \(final)")
        XCTAssertTrue(completed.waitForExistence(timeout: 3))
        assertVisible(completed)
    }

    func testAutomaticOutputPreservesHistoryAndNewTextReturnsToFinalReply() {
        submitStream()
        XCTAssertTrue(row("Dialogue. Peig. \(first). In progress").waitForExistence(timeout: 8))
        scroll.swipeDown(velocity: .fast)
        let historical = app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'transcript.item.' AND label BEGINSWITH 'Narration. Historical fixture entry'"
        )).allElementsBoundByIndex.first { $0.isHittable && $0.frame.minY >= scroll.frame.minY }
        guard let historical else { return XCTFail("No visible historical row after native swipe") }
        let identity = historical.identifier
        let y = historical.frame.minY
        XCTAssertTrue(app.buttons["composer.stop"].exists, "Stream must still be active when history anchor is recorded")
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 10))
        let same = app.descendants(matching: .any).matching(identifier: identity).firstMatch
        XCTAssertTrue(same.exists && same.isHittable)
        XCTAssertEqual(same.frame.minY, y, accuracy: 5)
        let newest = app.buttons["transcript.new-text"]
        XCTAssertTrue(newest.waitForExistence(timeout: 3))
        newest.tap()
        XCTAssertTrue(wait { !newest.exists })
        let completed = row("Dialogue. Peig. \(final)")
        XCTAssertTrue(completed.waitForExistence(timeout: 3))
        assertVisible(completed)
    }

    func testAutomaticStopRetainsExactPartialBeyondNextTimerTicks() {
        submitStream()
        let partial = row("Dialogue. Peig. \(first). In progress")
        XCTAssertTrue(partial.waitForExistence(timeout: 8))
        let identity = partial.identifier
        app.buttons["composer.stop"].tap()
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 3))
        let stopped = row("Dialogue. Peig. \(first). Interrupted; not applied")
        XCTAssertTrue(stopped.waitForExistence(timeout: 3))
        XCTAssertEqual(stopped.identifier, identity)
        let changed = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            !stopped.exists || stopped.identifier != identity || self.app.buttons["composer.stop"].exists
        }, object: nil)
        changed.isInverted = true
        XCTAssertEqual(XCTWaiter.wait(for: [changed], timeout: 6), .completed)
        input.tap()
        input.typeText("/look")
        XCTAssertEqual(input.value as? String, "/look")
        XCTAssertTrue(app.buttons["composer.send"].isEnabled)
    }

    func testLongFixtureTraversesOldestAndNewestWithinInteractionBudget() {
        XCTAssertTrue(scroll.waitForExistence(timeout: 5))
        let newest = row("Narration. Historical fixture entry 180.")
        XCTAssertTrue(newest.waitForExistence(timeout: 5))
        XCTAssertTrue(newest.isHittable)
        let oldest = row("Narration. Historical fixture entry 1.")
        let start = Date()
        for _ in 0..<35 {
            if oldest.exists && oldest.isHittable { break }
            scroll.swipeDown(velocity: .fast)
        }
        XCTAssertTrue(oldest.exists && oldest.isHittable)
        XCTAssertLessThan(Date().timeIntervalSince(start), 45,
                          "Audit bound: reaching oldest through fast native gestures")
        let end = Date()
        for _ in 0..<35 {
            if newest.exists && newest.isHittable { break }
            scroll.swipeUp(velocity: .fast)
        }
        XCTAssertTrue(newest.exists && newest.isHittable)
        XCTAssertLessThan(Date().timeIntervalSince(end), 45)
        input.tap()
        input.typeText("still responsive")
        XCTAssertEqual(input.value as? String, "still responsive")
    }

    func testNonTestFixtureRegionsAndBackgroundDraft() {
        XCTAssertTrue(app.otherElements["status.header"].waitForExistence(timeout: 5))
        XCTAssertTrue(scroll.exists)
        XCTAssertTrue(app.otherElements["composer"].exists)
        let header = app.otherElements["status.header"]
        XCTAssertGreaterThanOrEqual(header.frame.minY, app.frame.minY)
        XCTAssertLessThanOrEqual(input.frame.maxY, app.frame.maxY - 10)
        XCTAssertGreaterThanOrEqual(input.frame.minX, app.frame.minX)
        XCTAssertLessThanOrEqual(input.frame.maxX, app.frame.maxX)
        let statusBar = app.statusBars.firstMatch
        if statusBar.exists {
            XCTAssertGreaterThanOrEqual(header.frame.minY, statusBar.frame.maxY)
        }
        XCTAssertFalse(app.tabBars.firstMatch.exists)
        for id in ["fixture.step", "composer.diagnostics", "Map", "Save", "NPC sidebar", "Portrait", "Scene art"] {
            XCTAssertFalse(app.buttons[id].exists, "Forbidden control: \(id)")
        }
        let screenshot = XCTAttachment(screenshot: app.screenshot())
        screenshot.name = "Non-test fixture screen; supports hierarchy inspection only"
        screenshot.lifetime = .keepAlways
        add(screenshot)
        input.tap()
        input.typeText("a substantial unfinished command for background restoration")
        let draft = input.value as? String
        XCTAssertEqual(draft, "a substantial unfinished command for background restoration")
        XCUIDevice.shared.press(.home)
        XCTAssertTrue(app.wait(for: .runningBackground, timeout: 5))
        app.activate()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 5))
        XCTAssertEqual(input.value as? String, draft)
    }
}
