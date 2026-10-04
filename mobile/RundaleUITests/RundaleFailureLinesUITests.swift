import XCTest

/// A failed Endpoint call shows the mod's line for why it failed
/// (`mods/rundale/loading.toml` `[failure_lines]`), and Retry recovers. The
/// mock Endpoint fails "fail" requests as an unavailable provider and drops
/// the connection once for "offline once".
///
/// Set `TEST_RUNNER_RUNDALE_DEMO_HOLD=<seconds>` on `xcodebuild test` to hold
/// on each settled state when recording a demo (see the phase demo plan).
@MainActor
final class RundaleFailureLinesUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUp() async throws {
        try await super.setUp()
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments = ["--ui-tests", "--phase2", "--phase2-mock", "--no-auto-focus", "--reset-fixture"]
        app.launch()
        XCTAssertTrue(row(containing: "A muddy road runs between low stone walls")
            .waitForExistence(timeout: 15))
        submit("go to Connolly Cottage")
        XCTAssertTrue(row(containing: "A peat fire warms the single room").waitForExistence(timeout: 8))
    }

    func testAnUnavailableStorytellerHasItsOwnLine() {
        submit("ask Mícheál to fail")
        XCTAssertTrue(row(containing: "The storyteller has gone out to the bog and isn't back yet.")
            .waitForExistence(timeout: 10))
        XCTAssertFalse(row(containing: "could not be completed").exists, "Not the generic line")
        XCTAssertTrue(app.buttons["composer.retry"].waitForExistence(timeout: 5))
        hold(4)
    }

    func testALostConnectionHasItsOwnLineAndRetryRecovers() {
        submit("ask Mícheál offline once")
        XCTAssertTrue(row(containing: "The road out of the parish is washed away")
            .waitForExistence(timeout: 10))
        hold(3)
        let retry = app.buttons["composer.retry"]
        XCTAssertTrue(retry.waitForExistence(timeout: 5))
        retry.tap()
        XCTAssertTrue(row(containing: "difficult this week").waitForExistence(timeout: 20))
        XCTAssertFalse(retry.exists)
        hold(4)
    }

    private var input: XCUIElement {
        app.descendants(matching: .any).matching(identifier: "composer.input").firstMatch
    }

    private func submit(_ command: String) {
        XCTAssertTrue(input.waitForExistence(timeout: 5))
        input.tap()
        input.typeText(command)
        app.buttons["composer.send"].tap()
        let cleared = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value == '' OR value == nil"), object: input
        )
        XCTAssertEqual(XCTWaiter.wait(for: [cleared], timeout: 8), .completed, command)
    }

    private func row(containing text: String) -> XCUIElement {
        app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS %@", text
        )).firstMatch
    }

    /// Pauses for a human viewer when recording; a no-op in normal runs.
    private func hold(_ seconds: TimeInterval) {
        guard let value = ProcessInfo.processInfo.environment["RUNDALE_DEMO_HOLD"],
              let minimum = TimeInterval(value), minimum > 0 else { return }
        Thread.sleep(forTimeInterval: max(seconds, minimum))
    }
}
