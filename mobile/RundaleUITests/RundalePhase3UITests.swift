import XCTest

/// Canonical Phase 3 journey against the embedded Rust world and native UI.
/// The scenarios (clarification, unknown names, every Endpoint failing) are
/// tested in `EngineScenarioTests`.
@MainActor
class RundalePhase3UITestCase: RundaleUITestCase {
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

    func launch(reset: Bool) {
        app.launchArguments = ["--ui-tests", "--phase3", "--phase3-mock", "--no-auto-focus"]
        if reset { app.launchArguments.append("--reset-fixture") }
        app.launch()
    }

    func submit(_ command: String) {
        let input = app.descendants(matching: .any)
            .matching(identifier: "composer.input")
            .firstMatch
        XCTAssertTrue(input.waitForExistence(timeout: 5))
        input.tap()
        input.typeText(command)
        app.buttons["composer.send"].tap()
        let cleared = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value == '' OR value == nil"),
            object: input
        )
        XCTAssertEqual(XCTWaiter.wait(for: [cleared], timeout: 8), .completed)
    }

    func headerLabel(contains text: String) -> XCUIElement {
        app.otherElements.matching(
            NSPredicate(format: "identifier == 'status.header' AND label CONTAINS[c] %@", text)
        ).firstMatch
    }
}

/// Place-name travel with no Endpoint inference.
@MainActor
final class RundalePhase3UITests: RundalePhase3UITestCase {
    func testPlaceNameTravelWithoutEndpointInference() {
        launch(reset: true)
        XCTAssertTrue(headerLabel(contains: "Kilteevan Village").waitForExistence(timeout: 8))

        submit("go to the Letter Office")
        XCTAssertTrue(headerLabel(contains: "Letter Office").waitForExistence(timeout: 8))
        XCTAssertFalse(app.buttons["composer.stop"].exists)

        submit("go to Kilteevan Village")
        XCTAssertTrue(headerLabel(contains: "Kilteevan Village").waitForExistence(timeout: 8))
        XCTAssertFalse(app.buttons["composer.stop"].exists)
    }
}
