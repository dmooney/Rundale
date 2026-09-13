import XCTest

final class RundaleUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUp() {
        super.setUp()
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

    func testMultilineSubmissionClearsOnlyAcceptedDraft() {
        launch(fixture: "standard")

        let input = commandInput
        input.tap()
        input.typeText("look around\ncheck the road\nwait by the gate")
        XCTAssertTrue((input.value as? String)?.contains("check the road") == true)

        app.buttons["composer.send"].tap()
        XCTAssertTrue(waitForTranscriptText("look around\ncheck the road\nwait by the gate"))
        XCTAssertEqual(input.value as? String, "")
    }

    func testSendButtonExposesDistinctDisabledAndEnabledStates() {
        launch(fixture: "standard")

        let send = app.buttons["composer.send"]
        XCTAssertTrue(send.waitForExistence(timeout: 3))
        XCTAssertFalse(send.isEnabled)
        XCTAssertEqual(send.value as? String, "Enter a command to enable")

        commandInput.tap()
        commandInput.typeText("Look")

        XCTAssertTrue(send.isEnabled)
        XCTAssertEqual(send.value as? String, "Ready to send")
    }

    func testDraftTypedDuringAcceptedStreamSurvivesStop() {
        launch(fixture: "manual-stream")

        let input = commandInput
        input.tap()
        input.typeText("long stream")
        app.buttons["composer.send"].tap()
        XCTAssertTrue(app.buttons["fixture.step"].waitForExistence(timeout: 3))

        input.tap()
        input.typeText("draft typed while the response runs")
        app.buttons["composer.stop"].tap()

        XCTAssertEqual(input.value as? String, "draft typed while the response runs")
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

    func testFailedFixtureShowsErrorAndOffersRetry() {
        launch(fixture: "failed")

        let input = commandInput
        input.tap()
        input.typeText("fail")
        app.buttons["composer.send"].tap()
        finishManualStreamIfNeeded()

        XCTAssertTrue(waitForTranscriptText("failed before it could be applied"))
        let retry = app.buttons["composer.retry"]
        XCTAssertTrue(retry.waitForExistence(timeout: 3))
        retry.tap()
        XCTAssertTrue(app.buttons["fixture.step"].waitForExistence(timeout: 3))
        XCTAssertTrue(waitForTranscriptText("Retrying the request"))
        XCTAssertTrue(app.buttons["composer.stop"].waitForExistence(timeout: 3))
        XCTAssertFalse(app.buttons["composer.retry"].exists)
        XCTAssertFalse(app.buttons["composer.send"].exists)
        finishManualStreamIfNeeded()
        XCTAssertTrue(waitForTranscriptText("failed before it could be applied"))
    }

    func testAcceptedStreamIsInterruptedOnRelaunchAndRetryKeepsCommand() {
        launch(fixture: "restoration")

        let input = commandInput
        input.tap()
        input.typeText("long stream")
        app.buttons["composer.send"].tap()
        XCTAssertTrue(waitForTranscriptText("long stream"))

        let step = app.buttons["fixture.step"]
        XCTAssertTrue(step.waitForExistence(timeout: 3))
        step.tap()
        app.terminate()

        app = XCUIApplication()
        app.launchArguments = ["--ui-tests", "--no-auto-focus", "--fixture=restoration"]
        app.launch()

        XCTAssertTrue(waitForTranscriptText("long stream"))
        let retry = app.buttons["composer.retry"]
        XCTAssertTrue(retry.waitForExistence(timeout: 3))
        retry.tap()
        let retryStep = app.buttons["fixture.step"]
        XCTAssertTrue(retryStep.waitForExistence(timeout: 3))
        XCTAssertTrue(waitForTranscriptText("Retrying the request"))
        XCTAssertTrue(
            waitForValue("Retry accepted and persisted", on: retryStep),
            "Retry acceptance must reach durable presentation state before relaunch"
        )
        app.terminate()

        app = XCUIApplication()
        app.launchArguments = ["--ui-tests", "--no-auto-focus", "--fixture=restoration"]
        app.launch()

        XCTAssertTrue(waitForTranscriptText("Retrying the request"))
        XCTAssertTrue(waitForTranscriptText("Interrupted"))
        XCTAssertTrue(app.buttons["composer.retry"].waitForExistence(timeout: 3))
    }

    func testCompletionInsertionAndSlashCommandSubmission() {
        launch(fixture: "standard")

        let firstInput = commandInput
        firstInput.tap()
        firstInput.typeText("@")
        let npcCompletion = app.buttons["completion.npc-micheal"]
        XCTAssertTrue(npcCompletion.waitForExistence(timeout: 3))
        npcCompletion.tap()
        XCTAssertTrue((firstInput.value as? String)?.contains("Mícheál") == true)

        app.terminate()
        app = XCUIApplication()
        app.launchArguments = ["--ui-tests", "--no-auto-focus", "--reset-fixture", "--fixture=standard"]
        app.launch()
        let input = commandInput
        input.tap()
        input.typeText("/")
        let slashCompletion = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH 'completion.'")
        ).firstMatch
        XCTAssertTrue(slashCompletion.waitForExistence(timeout: 3))
        slashCompletion.tap()
        XCTAssertTrue((input.value as? String)?.hasPrefix("/") == true)
        app.buttons["composer.send"].tap()
        XCTAssertTrue(waitForTranscriptText("/"))
    }

    func testHistoryRecallCopiesAnEarlierCommandForEditing() {
        launch(fixture: "standard")

        submitAndFinish("look around")
        submitAndFinish("walk to the bridge")

        let history = app.buttons["composer.history"]
        XCTAssertTrue(history.waitForExistence(timeout: 3))
        history.tap()
        let input = commandInput
        XCTAssertEqual(input.value as? String, "walk to the bridge")
        input.typeText(" slowly")
        app.buttons["composer.send"].tap()
        XCTAssertTrue(waitForTranscriptText("walk to the bridge slowly"))
        XCTAssertTrue(waitForTranscriptText("walk to the bridge"))
    }

    func testFixtureSessionRestoresHistoryAndDraftAcrossRelaunch() {
        launch(fixture: "restoration")
        submitAndFinish("look around")

        let input = commandInput
        input.tap()
        input.typeText("an unsent note survives relaunch")
        app.terminate()

        app = XCUIApplication()
        app.launchArguments = ["--ui-tests", "--no-auto-focus", "--fixture=restoration"]
        app.launch()

        XCTAssertTrue(waitForTranscriptText("look around"))
        XCTAssertEqual(
            commandInput.value as? String,
            "an unsent note survives relaunch"
        )
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

    func testClarificationOptionIsTemporaryAndActionable() {
        launch(fixture: "clarification")

        let input = commandInput
        input.tap()
        input.typeText("ambiguous")
        app.buttons["composer.send"].tap()

        // UI tests run the fixture in deterministic manual-step mode. Advance
        // through interpretation and clarification before checking the
        // actionable prompt.
        let step = app.buttons["fixture.step"]
        XCTAssertTrue(step.waitForExistence(timeout: 3))
        step.tap()
        step.tap()
        let clarification = app.otherElements["clarification"]
        XCTAssertTrue(clarification.waitForExistence(timeout: 3))
        let option = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH 'clarification.option.'")
        ).firstMatch
        XCTAssertTrue(option.exists)
        let selectedName = option.label
        option.tap()
        XCTAssertTrue(app.otherElements["clarification"].waitForNonExistence(timeout: 3))
        XCTAssertTrue(waitForTranscriptText("Directed to \(selectedName)."))
        let resolvedPrompt = app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@", "I’m not sure which Connolly you mean.")
        ).firstMatch
        XCTAssertFalse(resolvedPrompt.exists)
    }

    func testReadingHistoryExposesNewTextAndReturnsToNewest() {
        launch(fixture: "long-history")

        let scroll = transcriptScroll
        XCTAssertTrue(scroll.waitForExistence(timeout: 3))

        let input = commandInput
        input.tap()
        input.typeText("continue the account")

        // Capture a row that is truly in the scroll viewport after the
        // keyboard is open. The same screen-space Y must survive streaming;
        // otherwise a missing bottom sentinel has silently jumped to latest.
        scroll.swipeDown(velocity: .fast)
        guard let historical = visibleHistoricalRow(in: scroll) else {
            XCTFail("No hittable historical row is visible after scrolling")
            return
        }
        let historicalLabel = historical.label
        let historicalY = historical.frame.minY

        // Growing the composer changes only the viewport. The passage being
        // read must retain the same screen-space position even when no new
        // transcript item arrives to trigger a reload.
        input.tap()
        input.typeText("\nwith another line\nand a third")
        let afterComposerGrowth = app.staticTexts[historicalLabel]
        XCTAssertTrue(afterComposerGrowth.waitForExistence(timeout: 3))
        XCTAssertLessThanOrEqual(abs(afterComposerGrowth.frame.minY - historicalY), 5)

        app.buttons["composer.send"].tap()

        let step = app.buttons["fixture.step"]
        XCTAssertTrue(step.waitForExistence(timeout: 3))
        step.tap()
        let newText = app.buttons["transcript.new-text"]
        XCTAssertTrue(newText.waitForExistence(timeout: 3))
        let sameHistorical = app.staticTexts[historicalLabel]
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

    func testFollowingNewestStartsAtLatestAndTracksComposerResize() {
        launch(fixture: "long-history")

        let scroll = transcriptScroll
        XCTAssertTrue(scroll.waitForExistence(timeout: 3))
        let latest = app.staticTexts.matching(
            NSPredicate(format: "label CONTAINS 'Historical fixture entry 180'")
        ).firstMatch
        XCTAssertTrue(waitForVisible(latest, in: scroll))

        let input = commandInput
        input.tap()
        input.typeText("first line\nsecond line\nthird line\nfourth line")
        XCTAssertTrue(waitForVisible(latest, in: scroll))
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

    private func visibleHistoricalRow(in scroll: XCUIElement) -> XCUIElement? {
        let candidates = app.staticTexts.matching(
            NSPredicate(format: "label CONTAINS 'Historical fixture entry'")
        )
        let scrollFrame = scroll.frame
        for index in 0..<candidates.count {
            let candidate = candidates.element(boundBy: index)
            guard candidate.isHittable, candidate.frame.intersects(scrollFrame) else { continue }
            return candidate
        }
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

    private func waitForVisible(_ element: XCUIElement,
                                in container: XCUIElement,
                                timeout: TimeInterval = 3) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            if element.exists,
               element.isHittable,
               element.frame.intersects(container.frame) {
                return true
            }
            RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.1))
        } while Date() < deadline
        return false
    }

    private func waitForValue(_ value: String,
                              on element: XCUIElement,
                              timeout: TimeInterval = 3) -> Bool {
        let predicate = NSPredicate(format: "value == %@", value)
        let expectation = XCTNSPredicateExpectation(predicate: predicate, object: element)
        return XCTWaiter.wait(for: [expectation], timeout: timeout) == .completed
    }
}

private extension XCUIElement {
    func waitForNonExistence(timeout: TimeInterval) -> Bool {
        let predicate = NSPredicate(format: "exists == false")
        let expectation = XCTNSPredicateExpectation(predicate: predicate, object: self)
        return XCTWaiter.wait(for: [expectation], timeout: timeout) == .completed
    }
}
