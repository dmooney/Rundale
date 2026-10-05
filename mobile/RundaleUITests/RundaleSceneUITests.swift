import XCTest

/// The engine's scenes on the canonical world, offline: the opening and an
/// arrival each render as one titled scene card with the place's description
/// and who is there, without the time, weather, or exits the header and
/// `/exits` already give. Movement is parsed locally, so no Endpoint is
/// called.
///
/// Set `TEST_RUNNER_RUNDALE_DEMO_HOLD=<seconds>` on `xcodebuild test` to hold
/// on each settled state when recording a demo (see the phase demo plan).
@MainActor
final class RundaleSceneUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUp() async throws {
        try await super.setUp()
        continueAfterFailure = false
        app = XCUIApplication()
    }

    func testOpeningAndArrivalAreTitledSceneCards() {
        app.launchArguments = ["--ui-tests", "--phase2", "--no-auto-focus", "--reset-fixture"]
        app.launch()

        let opening = sceneRow("Kilteevan Village", "A muddy road runs between low stone walls")
        XCTAssertTrue(opening.waitForExistence(timeout: 15), "Expected the opening scene card")
        assertNoRestatedHeader(opening.label)
        hold(3)

        let input = app.descendants(matching: .any).matching(identifier: "composer.input").firstMatch
        XCTAssertTrue(input.waitForExistence(timeout: 5))
        input.tap()
        input.typeText("go to Connolly Cottage")
        hold(2)
        app.buttons["composer.send"].tap()

        let arrival = sceneRow("Connolly Cottage", "A peat fire warms the single room")
        XCTAssertTrue(arrival.waitForExistence(timeout: 15), "Expected the arrival scene card")
        assertNoRestatedHeader(arrival.label)
        XCTAssertTrue(arrival.label.hasSuffix("are here."), arrival.label)
        XCTAssertFalse(
            transcriptText("You can go to").exists,
            "The transcript lists no exits on arrival"
        )
        hold(5)
    }

    /// Phase 4 test plan, arrival feedback: a place the player has been
    /// shows who is there without its description; an empty one is the
    /// title alone; `look` still describes it.
    func testReturningShowsWhoIsThereWithoutRepeatingTheDescription() {
        app.launchArguments = ["--ui-tests", "--phase2", "--no-auto-focus", "--reset-fixture"]
        app.launch()
        XCTAssertTrue(sceneRow("Kilteevan Village", "A muddy road").waitForExistence(timeout: 15))

        submit("go to Connolly Cottage")
        XCTAssertTrue(sceneRow("Connolly Cottage", "A peat fire warms the single room")
            .waitForExistence(timeout: 15))
        hold(3)

        // The opening described the village: coming back lists who is there.
        submit("go to Kilteevan Village")
        let villageReturn = app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'transcript.item.' AND label BEGINSWITH 'Scene. Kilteevan Village. ' AND label ENDSWITH 'is here.'"
        )).firstMatch
        XCTAssertTrue(villageReturn.waitForExistence(timeout: 15), "Expected a presence-only return")
        XCTAssertFalse(villageReturn.label.contains("muddy road"), villageReturn.label)
        hold(3)

        submit("go to the Letter Office")
        XCTAssertTrue(sceneRow("Letter Office", "A narrow counter").waitForExistence(timeout: 15))
        submit("go to Kilteevan Village")
        submit("go to the Letter Office")
        let emptyReturn = app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'transcript.item.' AND label == 'Scene. Letter Office'"
        )).firstMatch
        XCTAssertTrue(emptyReturn.waitForExistence(timeout: 15), "Expected the title alone")
        hold(3)

        submit("look")
        XCTAssertTrue(transcriptText("A narrow counter, pigeonholes").waitForExistence(timeout: 15))
        hold(4)
    }

    private func submit(_ command: String) {
        let input = app.descendants(matching: .any).matching(identifier: "composer.input").firstMatch
        XCTAssertTrue(input.waitForExistence(timeout: 5))
        input.tap()
        input.typeText(command)
        app.buttons["composer.send"].tap()
        let cleared = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value == '' OR value == nil"), object: input
        )
        XCTAssertEqual(XCTWaiter.wait(for: [cleared], timeout: 8), .completed)
    }

    /// A scene card's accessibility label is "Scene. <name>. <text>".
    private func sceneRow(_ name: String, _ text: String) -> XCUIElement {
        app.descendants(matching: .any).matching(
            NSPredicate(
                format: "identifier BEGINSWITH 'transcript.item.' AND label BEGINSWITH %@",
                "Scene. \(name). \(text)"
            )
        ).firstMatch
    }

    private func transcriptText(_ text: String) -> XCUIElement {
        app.descendants(matching: .any).matching(
            NSPredicate(format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS %@", text)
        ).firstMatch
    }

    private func assertNoRestatedHeader(_ label: String) {
        for restated in ["It is morning", "weather is", "sky hangs", "You can go to"] {
            XCTAssertFalse(label.contains(restated), "\(restated) in \(label)")
        }
    }
}
