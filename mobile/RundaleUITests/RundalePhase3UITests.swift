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

    func testArrivalsListPeopleAndKeepRepeatVisitsBriefAfterRelaunch() {
        launch(reset: true)
        submit("go west")
        XCTAssertTrue(headerLabel(contains: "Connolly Cottage").waitForExistence(timeout: 8))
        let description = "A peat fire warms the single room of Connolly Cottage."
        let firstDescription = rows(containing: description).firstMatch
        XCTAssertTrue(firstDescription.waitForExistence(timeout: 8))
        let originalDescriptionID = firstDescription.identifier
        let household = "Mícheál Connolly and Róisín Connolly are here."
        XCTAssertTrue(rows(containing: household).firstMatch.waitForExistence(timeout: 8))
        attach("First cottage visit describes the room and lists both people")

        submit("go east")
        submit("go east")
        XCTAssertTrue(headerLabel(contains: "Letter Office").waitForExistence(timeout: 8))
        XCTAssertTrue(rows(containing: "Peig Hannigan is here.").firstMatch.waitForExistence(timeout: 8))
        attach("Letter Office arrival lists Peig after her scheduled journey")
        app.terminate()
        app = XCUIApplication()
        launch(reset: false)
        XCTAssertTrue(headerLabel(contains: "Letter Office").waitForExistence(timeout: 8))

        submit("go west")
        submit("go west")
        XCTAssertTrue(headerLabel(contains: "Connolly Cottage").waitForExistence(timeout: 8))
        let returnPresence = rows(containing: household).matching(NSPredicate(
            format: "identifier != %@", originalDescriptionID
        )).firstMatch
        XCTAssertTrue(returnPresence.waitForExistence(timeout: 8))
        // Older rows may remain materialized. No newly appended description may appear.
        for row in rows(containing: description).allElementsBoundByIndex {
            XCTAssertEqual(row.identifier, originalDescriptionID)
        }
        attach("Returning after save resume lists people without repeating the description")

        submit("/look")
        XCTAssertTrue(rows(containing: "a peat fire, a scrubbed table, and rain-dark coats by the door.")
            .firstMatch.waitForExistence(timeout: 8))
        attach("Explicit look still describes a familiar room")
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
            predicate: NSPredicate(format: "value == '' OR value == nil"),
            object: input
        )
        XCTAssertEqual(XCTWaiter.wait(for: [cleared], timeout: 8), .completed)
    }

    private func headerLabel(contains text: String) -> XCUIElement {
        app.otherElements.matching(
            NSPredicate(format: "identifier == 'status.header' AND label CONTAINS[c] %@", text)
        ).firstMatch
    }

    private func rows(containing text: String) -> XCUIElementQuery {
        app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS %@", text
        ))
    }

    private func attach(_ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    private func waitForText(_ text: String, timeout: TimeInterval) -> Bool {
        app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@", text)
        ).firstMatch.waitForExistence(timeout: timeout)
    }
}
