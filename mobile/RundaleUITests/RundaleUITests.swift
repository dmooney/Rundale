import XCTest

@MainActor
final class RundaleUITests: RundaleUITestCase {
    private var app: XCUIApplication!

    override func setUp() async throws {
        try await super.setUp()
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments = ["--ui-tests", "--no-auto-focus", "--reset-fixture"]
    }

    func testLaunchShowsOnlyPrimaryPlayRegions() {
        launch(fixture: "standard")

        XCTAssertTrue(app.otherElements["status.header"].waitForExistence(timeout: 3))
        XCTAssertTrue(transcriptRegion.waitForExistence(timeout: 3))
        XCTAssertTrue(app.otherElements["composer"].exists)
        XCTAssertFalse(app.tabBars.firstMatch.exists)
        XCTAssertFalse(app.buttons["Map"].exists)
        XCTAssertFalse(app.buttons["Save"].exists)
    }

    func testManualStreamCanBeStoppedWithPartialOutputRetained() {
        launch(fixture: "manual-stream")

        let input = commandInput
        input.tap()
        input.typeText("long stream")
        app.buttons["composer.send"].tap()

        let step = app.buttons["fixture.step"]
        XCTAssertTrue(step.waitForExistence(timeout: 3))
        step.tap()
        step.tap()
        app.buttons["composer.stop"].tap()

        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 3))
        XCTAssertTrue(waitForTranscriptText("Interrupted"))
    }

    func testOrdinaryLaunchRestoresHistoryAndDraftAcrossRelaunch() {
        launch(fixture: "standard")
        submitAndFinish("look around")

        let input = commandInput
        input.tap()
        input.typeText("ordinary launch keeps this note")
        app.terminate()

        // The second launch uses the normal fixture and omits both the
        // restoration fixture alias and reset flag. Local session recovery is
        // therefore exercised through the product launch path itself.
        app = XCUIApplication()
        app.launchArguments = ["--ui-tests", "--no-auto-focus", "--fixture=standard"]
        app.launch()

        XCTAssertTrue(waitForTranscriptText("look around"))
        XCTAssertEqual(
            commandInput.value as? String,
            "ordinary launch keeps this note"
        )
    }

    func testReadingHistoryExposesNewTextAndReturnsToNewest() {
        launch(fixture: "long-history")

        let scroll = transcriptScroll
        XCTAssertTrue(scroll.waitForExistence(timeout: 3))

        let input = commandInput
        input.tap()
        input.typeText("continue the account")
        app.buttons["composer.send"].tap()
        let step = app.buttons["fixture.step"]
        XCTAssertTrue(step.waitForExistence(timeout: 3))

        // Capture a row that is truly in the scroll viewport after the
        // accepted Send has returned to the live exchange. An intentional
        // history gesture during the response must survive later streaming;
        // otherwise a missing bottom sentinel has silently jumped to latest.
        guard let newestVisible = visibleHistoricalRow(in: scroll) else {
            XCTFail("No transcript row is visible before scrolling")
            return
        }
        // XCUIElement queries resolve lazily; capture the value before the
        // gesture rather than re-resolving the first visible row afterward.
        let newestIdentifier = newestVisible.identifier
        scroll.swipeDown(velocity: .fast)
        guard let historical = visibleHistoricalRow(in: scroll) else {
            XCTFail("No hittable historical row is visible after scrolling")
            return
        }
        XCTAssertNotEqual(
            historical.identifier,
            newestIdentifier,
            "A history gesture must change the visible transcript rows"
        )
        let historicalIdentifier = historical.identifier
        let historicalY = historical.frame.minY

        step.tap()
        let newText = app.buttons["transcript.new-text"]
        XCTAssertTrue(newText.waitForExistence(timeout: 3))
        let sameHistorical = app.staticTexts[historicalIdentifier]
        XCTAssertTrue(sameHistorical.waitForExistence(timeout: 3))
        XCTAssertTrue(sameHistorical.isHittable)
        XCTAssertTrue(sameHistorical.frame.intersects(scroll.frame))
        XCTAssertLessThanOrEqual(abs(sameHistorical.frame.minY - historicalY), 5)
        newText.tap()
        XCTAssertTrue(newText.waitForNonExistence(timeout: 3))
        let latest = app.staticTexts.matching(
            NSPredicate(format: "label CONTAINS %@", "I’ll consider that.")
        ).firstMatch
        XCTAssertTrue(latest.waitForExistence(timeout: 3))
        XCTAssertTrue(latest.frame.intersects(scroll.frame))
    }

    func testReadingAnchorRestoresTheVisibleHistoricalRow() {
        launch(fixture: "restoration-long-history")

        let scroll = transcriptScroll
        XCTAssertTrue(scroll.waitForExistence(timeout: 3))
        scroll.swipeDown(velocity: .fast)

        guard let historical = visibleHistoricalRow(in: scroll) else {
            XCTFail("No hittable historical row is visible after scrolling")
            return
        }
        let historicalLabel = historical.label
        let historicalViewportY = historical.frame.minY - scroll.frame.minY
        app.terminate()

        app = XCUIApplication()
        app.launchArguments = ["--ui-tests", "--no-auto-focus", "--fixture=restoration-long-history"]
        app.launch()

        let restored = app.staticTexts[historicalLabel]
        XCTAssertTrue(restored.waitForExistence(timeout: 3))
        XCTAssertTrue(restored.isHittable)
        XCTAssertTrue(restored.frame.intersects(transcriptScroll.frame))
        let restoredViewportY = restored.frame.minY - transcriptScroll.frame.minY
        XCTAssertLessThanOrEqual(
            abs(restoredViewportY - historicalViewportY),
            5,
            "The saved logical row should retain its viewport-relative offset"
        )
    }

    func testCoreAccessibilityLabelsRemainMeaningful() {
        launch(fixture: "standard")

        let header = app.otherElements["status.header"]
        XCTAssertTrue(header.waitForExistence(timeout: 3))
        XCTAssertTrue(header.label.contains("Current place"))
        XCTAssertTrue(commandInput.label.contains("Command draft"))
        XCTAssertTrue(app.buttons["composer.send"].label.contains("Send command"))
        XCTAssertTrue(app.buttons["composer.send"].isHittable)
    }

    func testAccessibilitySizeKeepsCoreControlsAvailable() {
        app.launchArguments.append(contentsOf: [
            "-UIPreferredContentSizeCategoryName",
            "UICTContentSizeCategoryAccessibilityXXXL"
        ])
        launch(fixture: "standard")

        let header = app.otherElements["status.header"]
        XCTAssertTrue(header.waitForExistence(timeout: 3))
        XCTAssertTrue((header.value as? String)?.localizedCaseInsensitiveContains("accessibility") == true)
        XCTAssertTrue(commandInput.waitForExistence(timeout: 3))
        XCTAssertTrue(app.buttons["composer.send"].isHittable)
    }

    func testDarkAppearanceKeepsCoreControlsAvailable() {
        app.launchArguments.append("--force-dark-appearance")
        launch(fixture: "standard")

        let header = app.otherElements["status.header"]
        XCTAssertTrue(header.waitForExistence(timeout: 3))
        XCTAssertTrue((header.value as? String)?.localizedCaseInsensitiveContains("dark") == true)
        XCTAssertTrue(commandInput.waitForExistence(timeout: 3))
        XCTAssertTrue(app.buttons["composer.send"].isHittable)
    }

    private func launch(fixture: String) {
        app.launchArguments.append("--fixture=\(fixture)")
        app.launch()
    }

    /// The Phase 1 composer deliberately remains a native text control. Its
    /// concrete AX type can be `TextField` for a vertical-axis TextField or
    /// `TextView` for a future measured UITextView implementation, so tests
    /// address the stable identifier rather than coupling to either wrapper.
    private var commandInput: XCUIElement {
        app.descendants(matching: .any)
            .matching(identifier: "composer.input")
            .firstMatch
    }

    private var transcriptRegion: XCUIElement {
        app.descendants(matching: .any)
            .matching(identifier: "transcript")
            .firstMatch
    }

    private var transcriptScroll: XCUIElement {
        app.descendants(matching: .any)
            .matching(identifier: "transcript")
            .firstMatch
    }

    /// One historical row as the accessibility tree reported it at a single
    /// instant. Equal snapshots on consecutive polls mean the scroll stopped.
    private struct HistoricalRowSnapshot: Equatable {
        let label: String
        let frame: CGRect
    }

    /// The historical rows currently inside `scrollFrame`, top to bottom, read
    /// from one accessibility snapshot. A snapshot is one consistent read of the
    /// tree, so a lazy row recycled mid-scroll cannot vanish between reading its
    /// label and its frame, and no live element is asked for `isHittable` (which
    /// throws for an element without a valid frame or activation point).
    private func historicalRowSnapshots(in scrollFrame: CGRect) -> [HistoricalRowSnapshot] {
        guard let root = try? app.snapshot() else { return [] }
        var rows: [HistoricalRowSnapshot] = []
        func visit(_ node: XCUIElementSnapshot) {
            if node.elementType == .staticText,
               node.label.contains("Historical fixture entry"),
               node.frame.intersects(scrollFrame) {
                rows.append(HistoricalRowSnapshot(label: node.label, frame: node.frame))
            }
            node.children.forEach(visit)
        }
        visit(root)
        return rows.sorted { ($0.frame.minY, $0.label) < ($1.frame.minY, $1.label) }
    }

    /// Waits until the historical rows inside the transcript stop moving (two
    /// consecutive snapshots match), then returns the first one that is hittable.
    /// A fast swipe keeps the transcript decelerating and the lazy stack
    /// recycling rows after `swipeDown` returns; asking a row mid-flight for
    /// `isHittable` throws "Activation point invalid" (#2314).
    private func visibleHistoricalRow(in scroll: XCUIElement, timeout: TimeInterval = 5) -> XCUIElement? {
        let scrollFrame = scroll.frame
        let deadline = Date().addingTimeInterval(timeout)
        var previous: [HistoricalRowSnapshot] = []
        repeat {
            let current = historicalRowSnapshots(in: scrollFrame)
            if !current.isEmpty, current == previous {
                for row in current {
                    let element = app.staticTexts[row.label]
                    if element.exists, element.isHittable { return element }
                }
                return nil
            }
            previous = current
            RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.1))
        } while Date() < deadline
        return nil
    }

    private func submitAndFinish(_ command: String) {
        let input = commandInput
        input.tap()
        input.typeText(command)
        app.buttons["composer.send"].tap()
        XCTAssertTrue(waitForTranscriptText(command))
        finishManualStreamIfNeeded()
    }

    private func finishManualStreamIfNeeded() {
        let step = app.buttons["fixture.step"]
        for _ in 0..<12 {
            guard step.exists else { return }
            step.tap()
        }
        XCTAssertFalse(step.exists, "Fixture did not reach a terminal state")
    }

    private func waitForTranscriptText(_ text: String,
                                       timeout: TimeInterval = 3) -> Bool {
        let matching = app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@", text)
        ).firstMatch
        return matching.waitForExistence(timeout: timeout)
    }
}

private extension XCUIElement {
    func waitForNonExistence(timeout: TimeInterval) -> Bool {
        let predicate = NSPredicate(format: "exists == false")
        let expectation = XCTNSPredicateExpectation(predicate: predicate, object: self)
        return XCTWaiter.wait(for: [expectation], timeout: timeout) == .completed
    }
}
