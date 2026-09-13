import XCTest

/// Canonical Phase 3 checks against the embedded Rust world and native UI.
@MainActor
final class RundalePhase3UITests: XCTestCase {
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

    func testVisitsCanonicalWorldAndShowsAuthoritativePresence() {
        launch(reset: true)
        XCTAssertTrue(headerLabel(contains: "Kilteevan Village").waitForExistence(timeout: 8))

        submit("walk to Connolly Cottage")
        XCTAssertTrue(headerLabel(contains: "Connolly Cottage").waitForExistence(timeout: 8))
        submit("/people")
        XCTAssertTrue(waitForText("Mícheál Connolly", timeout: 8))
        XCTAssertTrue(waitForText("Róisín Connolly", timeout: 8))

        submit("go to Kilteevan Village")
        XCTAssertTrue(headerLabel(contains: "Kilteevan Village").waitForExistence(timeout: 8))
        submit("go to Letter Office")
        XCTAssertTrue(headerLabel(contains: "Letter Office").waitForExistence(timeout: 8))
        submit("/people")
        XCTAssertTrue(waitForText("Peig Hannigan", timeout: 8))
    }

    func testExitDirectionsTravelWithoutEndpointInference() {
        launch(reset: true)
        XCTAssertTrue(headerLabel(contains: "Kilteevan Village").waitForExistence(timeout: 8))

        submit("go east")
        XCTAssertTrue(headerLabel(contains: "Letter Office").waitForExistence(timeout: 8))
        XCTAssertFalse(app.buttons["composer.stop"].exists)

        submit("go west")
        XCTAssertTrue(headerLabel(contains: "Kilteevan Village").waitForExistence(timeout: 8))
        XCTAssertFalse(app.buttons["composer.stop"].exists)
    }

    func testAmbiguousConnollyRequiresSelectionBeforeEndpointWork() {
        launch(reset: true)
        submit("/go Connolly Cottage")
        submit("ask Connolly about the household")

        XCTAssertTrue(app.otherElements["clarification"].waitForExistence(timeout: 8))
        XCTAssertTrue(app.buttons["clarification.option.choose-npc-micheal"].waitForExistence(timeout: 3))
        let roisin = app.buttons["clarification.option.choose-npc-roisin"]
        XCTAssertTrue(roisin.waitForExistence(timeout: 3))
        let unresolvedPrompt = app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@", "Which person do you mean?")
        ).firstMatch
        XCTAssertTrue(unresolvedPrompt.exists)
        XCTAssertFalse(app.buttons["composer.stop"].exists)
        roisin.tap()

        XCTAssertTrue(waitForText("Róisín Connolly", timeout: 8))
        XCTAssertFalse(app.otherElements["clarification"].exists)
        XCTAssertTrue(unresolvedPrompt.waitForNonExistence(timeout: 3))
        XCTAssertTrue(waitForText("household work allows", timeout: 15))
    }

    func testTaggedNearbyPersonBypassesClarification() {
        launch(reset: true)
        submit("/go Connolly Cottage")

        let input = app.descendants(matching: .any)
            .matching(identifier: "composer.input")
            .firstMatch
        input.tap()
        input.typeText("Hello @Mich")
        let micheal = app.buttons["completion.npc-micheal"]
        XCTAssertTrue(micheal.waitForExistence(timeout: 3))
        micheal.tap()
        XCTAssertEqual(input.value as? String, "Hello @Mícheál Connolly")
        app.buttons["composer.send"].tap()

        XCTAssertFalse(app.otherElements["clarification"].waitForExistence(timeout: 2))
        XCTAssertTrue(waitForText("moving cattle", timeout: 15))
    }

    func testTravelAndPresenceRestoreAfterRelaunch() {
        launch(reset: true)
        submit("go to Connolly Cottage")
        XCTAssertTrue(headerLabel(contains: "Connolly Cottage").waitForExistence(timeout: 8))
        app.terminate()

        app = XCUIApplication()
        launch(reset: false)
        XCTAssertTrue(headerLabel(contains: "Connolly Cottage").waitForExistence(timeout: 8))
        submit("/people")
        XCTAssertTrue(waitForText("Mícheál Connolly", timeout: 8))
        XCTAssertTrue(waitForText("Róisín Connolly", timeout: 8))
    }

    private func launch(reset: Bool) {
        app.launchArguments = ["--ui-tests", "--phase3", "--phase3-mock", "--no-auto-focus"]
        if reset { app.launchArguments.append("--reset-fixture") }
        app.launch()
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

    private func headerLabel(contains text: String) -> XCUIElement {
        app.otherElements.matching(
            NSPredicate(format: "identifier == 'status.header' AND label CONTAINS[c] %@", text)
        ).firstMatch
    }

    private func waitForText(_ text: String, timeout: TimeInterval) -> Bool {
        app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@", text)
        ).firstMatch.waitForExistence(timeout: timeout)
    }
}
