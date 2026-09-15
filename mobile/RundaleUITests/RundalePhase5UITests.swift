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

    private func waitForEndpointCompletion(timeout: TimeInterval = 8) {
        XCTAssertTrue(app.buttons["composer.stop"].waitForNonExistence(timeout: timeout))
    }
}
