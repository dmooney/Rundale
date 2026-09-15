import XCTest

/// Production-path native proofs for the five bounded Phase 5 mechanisms.
/// Typed Rust diagnostics are asserted alongside transcript behavior; these
/// simulator cases do not replace the signed-iPhone acceptance session.
@MainActor
final class RundalePhase5UITests: XCTestCase {
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

    func testPeigAloneRemembersAthleagueAfterRelaunch() {
        launch(reset: true)
        submit("I grew up in Athleague.")
        XCTAssertTrue(waitForText("I'll remember what you told me", timeout: 15))
        waitForEndpointCompletion()
        submit("/debug memory Peig")
        XCTAssertTrue(waitForText("player_claim", timeout: 5))
        XCTAssertTrue(waitForText("Athleague", timeout: 5))

        app.terminate()
        app = XCUIApplication()
        launch(reset: false)
        submit("/debug memory Peig")
        XCTAssertTrue(waitForText("player_claim", timeout: 5))
        submit("/debug memory Mícheál")
        XCTAssertTrue(waitForText(#""recordCount" : 0"#, timeout: 5))
    }

    func testRoisonGainsOnlyAuthoredGossipAfterContact() {
        launch(reset: true)
        submit("/debug knowledge Róisín")
        XCTAssertTrue(waitForText(#""recordCount" : 0"#, timeout: 5))
        submit("/wait 1")
        XCTAssertTrue(waitForText("Róisín hears Mícheál", timeout: 5))
        submit("/debug knowledge Róisín")
        XCTAssertTrue(waitForText("heard_from_npc", timeout: 5))
        XCTAssertTrue(waitForText("fact-micheal-stock", timeout: 5))
        submit("/debug knowledge Peig")
        XCTAssertTrue(waitForText(#""recordCount" : 0"#, timeout: 5))
    }

    func testLetterTaskPersistsAcrossAllTransitions() {
        launch(reset: true)
        submit("go to Letter Office")
        submit("/wait 10")
        submit("Peig, do you have work for me?")
        XCTAssertTrue(waitForText("small delivery", timeout: 15))
        waitForEndpointCompletion()
        submit("/debug tasks")
        XCTAssertTrue(waitForText("assigned", timeout: 5))
        submit("take the sealed letter")
        submit("/debug tasks")
        XCTAssertTrue(waitForText("in_progress", timeout: 5))

        app.terminate()
        app = XCUIApplication()
        launch(reset: false)
        submit("go to Kilteevan Village")
        submit("go to Connolly Cottage")
        submit("give the letter to Róisín")
        XCTAssertTrue(waitForText("accepts the sealed letter", timeout: 8))
        submit("/debug tasks")
        XCTAssertTrue(waitForText("completed", timeout: 5))
        submit("go to Kilteevan Village")
        submit("go to Letter Office")
        submit("Peig, was the letter delivered?")
        XCTAssertTrue(waitForText("delivered the letter", timeout: 15))
        waitForEndpointCompletion()
    }

    func testMichealClearAndHeavyRainDecisionsExposeCauses() {
        launch(reset: true)
        submit("/setup time 09:59")
        submit("/setup weather Clear")
        submit("/wait 1")
        submit("/debug world")
        XCTAssertTrue(waitForText(#""cause" : "schedule""#, timeout: 5))
        XCTAssertTrue(waitForText("kilteevan-village", timeout: 5))

        submit("/setup reset CONFIRM")
        submit("/setup time 09:59")
        submit("/setup weather Heavy Rain")
        submit("/wait 1")
        submit("/debug world")
        XCTAssertTrue(waitForText("weather_override", timeout: 5))
        XCTAssertTrue(waitForText("connolly-cottage", timeout: 5))
    }

    func testPeigSchedulePresenceAndConversationSurviveRelaunch() {
        launch(reset: true)
        submit("/wait 2")
        submit("go to Letter Office")
        submit("/people")
        XCTAssertTrue(waitForText("Peig Hannigan", timeout: 8))
        submit("/debug world")
        XCTAssertTrue(waitForText(#""cause" : "schedule""#, timeout: 5))

        app.terminate()
        app = XCUIApplication()
        launch(reset: false)
        submit("Peig, are you open?")
        XCTAssertTrue(waitForText("Peig Hannigan", timeout: 15))
    }

    func testWaitAdvancesWorldAndRejectsOutOfRangeMinutes() {
        launch(reset: true)
        submit("/wait 15")
        XCTAssertTrue(waitForText("The world advances by 15 minutes", timeout: 5))
        XCTAssertTrue(waitForText("You wait for 15 minutes", timeout: 5))

        submit("/wait 0")
        XCTAssertTrue(waitForText("Use /wait with 1 to 1440 minutes", timeout: 5))
    }

    func testDiagnosticsStayReadOnlyAndSetupRecordsProvenance() {
        launch(reset: true)
        submit("/debug world")
        let worldRevision = stateRevision(in: labelContaining(#""kind" : "world""#))

        submit("/debug memory Peig")
        XCTAssertEqual(
            stateRevision(in: labelContaining(#""kind" : "memory""#)),
            worldRevision
        )
        submit("/debug knowledge Róisín")
        XCTAssertEqual(
            stateRevision(in: labelContaining(#""kind" : "knowledge""#)),
            worldRevision
        )
        submit("/debug tasks")
        XCTAssertEqual(
            stateRevision(in: labelContaining(#""kind" : "tasks""#)),
            worldRevision
        )

        submit("/setup time 09:59")
        XCTAssertTrue(waitForText("INTERNAL SETUP", timeout: 5))
        XCTAssertTrue(waitForText(#""override" : "time""#, timeout: 5))
        submit("/setup location Letter Office")
        XCTAssertTrue(waitForText(#""override" : "location""#, timeout: 5))
        submit("/setup weather Heavy Rain")
        XCTAssertTrue(waitForText(#""override" : "weather""#, timeout: 5))
        submit("/debug world")
        XCTAssertTrue(waitForText("diagnosticOverrides", timeout: 5))

        let resetInput = app.descendants(matching: .any)
            .matching(identifier: "composer.input")
            .firstMatch
        resetInput.tap()
        resetInput.typeText("/setup reset")
        app.buttons["composer.send"].tap()
        XCTAssertTrue(waitForText("requires explicit confirmation", timeout: 5))
        XCTAssertEqual(resetInput.value as? String, "/setup reset")
        resetInput.tap()
        resetInput.typeText(" CONFIRM")
        app.buttons["composer.send"].tap()
        XCTAssertTrue(waitForText(#""override" : "reset""#, timeout: 5))
    }

    func testCancelledMemoryCandidateHasNoEffectAfterRelaunch() {
        launch(reset: true)
        submit("I grew up in Athleague. slow")
        let stop = app.buttons["composer.stop"]
        XCTAssertTrue(stop.waitForExistence(timeout: 5))
        stop.tap()
        XCTAssertTrue(stop.waitForNonExistence(timeout: 5))

        submit("/debug memory Peig")
        XCTAssertTrue(waitForText(#""recordCount" : 0"#, timeout: 5))

        app.terminate()
        app = XCUIApplication()
        launch(reset: false)
        submit("/debug memory Peig")
        XCTAssertTrue(waitForText(#""recordCount" : 0"#, timeout: 5))
    }

    private func launch(reset: Bool) {
        app.launchArguments = [
            "--ui-tests", "--phase3", "--phase3-mock", "--no-auto-focus"
        ]
        if reset { app.launchArguments.append("--reset-fixture") }
        app.launch()
        XCTAssertTrue(app.otherElements["status.header"].waitForExistence(timeout: 8))
    }

    private func submit(_ command: String) {
        let input = app.descendants(matching: .any)
            .matching(identifier: "composer.input")
            .firstMatch
        XCTAssertTrue(input.waitForExistence(timeout: 5))
        input.tap()
        input.typeText(command)
        app.buttons["composer.send"].tap()
        let cleared = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value == %@", ""),
            object: input
        )
        XCTAssertEqual(XCTWaiter.wait(for: [cleared], timeout: 8), .completed)
    }

    private func waitForText(_ text: String, timeout: TimeInterval) -> Bool {
        app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@", text)
        ).firstMatch.waitForExistence(timeout: timeout)
    }

    private func labelContaining(_ text: String) -> String {
        let element = app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@", text)
        ).firstMatch
        XCTAssertTrue(element.waitForExistence(timeout: 5))
        return element.label
    }

    private func stateRevision(in label: String) -> String? {
        let expression = try? NSRegularExpression(
            pattern: #""stateRevision"\s*:\s*(\d+)"#
        )
        let range = NSRange(label.startIndex..., in: label)
        guard let match = expression?.firstMatch(in: label, range: range),
              let revisionRange = Range(match.range(at: 1), in: label) else {
            return nil
        }
        return String(label[revisionRange])
    }

    private func waitForEndpointCompletion(timeout: TimeInterval = 8) {
        XCTAssertTrue(app.buttons["composer.stop"].waitForNonExistence(timeout: timeout))
    }
}
