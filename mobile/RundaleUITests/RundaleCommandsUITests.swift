import XCTest

/// Product spec §5.4 on the shared engine (#2120): the advertised slash
/// commands are offered under Commands and answer locally, with no Endpoint
/// call; the unadvertised time and inspection commands also work.
///
/// Set `TEST_RUNNER_RUNDALE_DEMO_HOLD=<seconds>` on `xcodebuild test` to hold
/// on each settled state when recording a demo (see the phase demo plan).
final class RundaleCommandsUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUp() {
        super.setUp()
        continueAfterFailure = false
        app = XCUIApplication()
        // No Endpoint is configured: a command that reached inference would
        // fail instead of answering.
        app.launchArguments = ["--ui-tests", "--phase2", "--no-auto-focus", "--reset-fixture"]
        app.launch()
        XCTAssertTrue(row(containing: "A muddy road runs between low stone walls")
            .waitForExistence(timeout: 15))
    }

    func testAdvertisedCommandsAreOfferedAndAnswerLocally() {
        let commands = app.buttons["composer.commands"]
        XCTAssertTrue(commands.waitForExistence(timeout: 5), "The Commands button is shown")
        commands.tap()
        for name in ["look", "people", "exits", "help"] {
            XCTAssertTrue(app.buttons["completion.\(name)"].waitForExistence(timeout: 3), name)
        }
        hold(3)
        app.buttons["completion.exits"].tap()
        XCTAssertEqual(input.value as? String, "/exits")
        app.buttons["composer.send"].tap()
        XCTAssertTrue(row(containing: "Connolly Cottage (4 min on foot)").waitForExistence(timeout: 8))
        hold(3)

        submit("/help")
        XCTAssertTrue(row(containing: "/people: Who is here").waitForExistence(timeout: 8))
        XCTAssertFalse(row(containing: "/wait").exists, "/wait is not advertised")

        submit("go to Connolly Cottage")
        submit("/people")
        XCTAssertTrue(row(containing: "cattle drover").waitForExistence(timeout: 8))
        submit("/look")
        XCTAssertTrue(row(containing: "A peat fire warms the single room. A scrubbed table")
            .waitForExistence(timeout: 8))
        XCTAssertFalse(app.buttons["composer.retry"].exists, "No command failed")
        hold(4)
    }

    func testUnadvertisedCommandsWorkAndOthersAreRefused() {
        submit("/wait 60")
        XCTAssertTrue(row(containing: "You wait for 60 minutes").waitForExistence(timeout: 8))
        submit("/debug clock")
        XCTAssertTrue(row(containing: "[DEBUG CLOCK]").waitForExistence(timeout: 8))
        hold(3)

        input.tap()
        input.typeText("/save")
        app.buttons["composer.send"].tap()
        XCTAssertTrue(app.descendants(matching: .any).matching(NSPredicate(
            format: "label CONTAINS %@", "That command isn't available here. Try /help."
        )).firstMatch.waitForExistence(timeout: 8))
        XCTAssertEqual(input.value as? String, "/save", "A refused command keeps the draft")
        hold(3)
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
