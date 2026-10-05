import XCTest

/// Canonical Phase 3 checks against the embedded Rust world and native UI.
@MainActor
class RundalePhase3UITestCase: XCTestCase {
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

    /// Launches the real Endpoint client (no mock transport) against a base
    /// URL that never resolves, so every Endpoint request fails as it would
    /// with the network down.
    func launchWithUnreachableEndpoint() {
        app.launchArguments = ["--ui-tests", "--phase3", "--no-auto-focus", "--reset-fixture"]
        app.launchEnvironment["RUNDALE_ENDPOINT_BASE_URL"] = "https://endpoints.invalid"
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

    func rows(containing text: String) -> XCUIElementQuery {
        app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS %@", text
        ))
    }

    /// Pauses for a human viewer when recording; a no-op in normal runs.
    func hold(_ seconds: TimeInterval) {
        guard let value = ProcessInfo.processInfo.environment["RUNDALE_DEMO_HOLD"],
              let minimum = TimeInterval(value), minimum > 0 else { return }
        Thread.sleep(forTimeInterval: max(seconds, minimum))
    }

    func attach(_ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    func waitForText(_ text: String, timeout: TimeInterval) -> Bool {
        app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@", text)
        ).firstMatch.waitForExistence(timeout: timeout)
    }
}

/// Canonical world visit and place-name travel.
@MainActor
final class RundalePhase3UITests: RundalePhase3UITestCase {
    func testVisitsCanonicalWorldAndShowsAuthoritativePresence() {
        launch(reset: true)
        XCTAssertTrue(headerLabel(contains: "Kilteevan Village").waitForExistence(timeout: 8))

        submit("walk to Connolly Cottage")
        XCTAssertTrue(headerLabel(contains: "Connolly Cottage").waitForExistence(timeout: 8))
        submit("/people")
        XCTAssertTrue(waitForText("Smallholder and cattle drover", timeout: 8))
        XCTAssertTrue(waitForText("Spinner and household bookkeeper", timeout: 8))

        submit("go to Kilteevan Village")
        XCTAssertTrue(headerLabel(contains: "Kilteevan Village").waitForExistence(timeout: 8))
        submit("go to the Letter Office")
        XCTAssertTrue(headerLabel(contains: "Letter Office").waitForExistence(timeout: 8))
        submit("/people")
        XCTAssertTrue(waitForText("No one else is here.", timeout: 8))

        // Peig's schedule has her set out for the Letter Office at 09:00. A
        // long wait moves the clock in one step, so she only sets off when it
        // ends; a short one then lets her arrive (as the canonical world
        // sheet's script does).
        submit("/wait 120")
        submit("/wait 10")
        submit("/people")
        XCTAssertTrue(waitForText("Letter-office keeper", timeout: 8))
    }

    /// Spec Milestones 2 and 3: gameplay that needs no inference works with
    /// every Endpoint request failing, and a failed conversation leaves the
    /// session playable.
    func testLocalCommandsAndTravelWorkWhenEveryEndpointRequestFails() {
        launchWithUnreachableEndpoint()
        XCTAssertTrue(headerLabel(contains: "Kilteevan Village").waitForExistence(timeout: 15))
        hold(3)

        submit("/look")
        XCTAssertTrue(waitForText("A muddy road runs between low stone walls", timeout: 8))
        hold(3)
        submit("/exits")
        XCTAssertTrue(waitForText("Connolly Cottage (4 min on foot)", timeout: 8))
        hold(3)
        submit("go to Connolly Cottage")
        XCTAssertTrue(headerLabel(contains: "Connolly Cottage").waitForExistence(timeout: 8))
        submit("/people")
        XCTAssertTrue(waitForText("Smallholder and cattle drover", timeout: 8))
        XCTAssertFalse(app.buttons["composer.retry"].exists, "No local command needed an Endpoint")
        hold(4)

        submit("ask Mícheál about the cattle")
        XCTAssertTrue(waitForText("The road out of the parish is washed away", timeout: 30),
                      "The mod's offline line")
        XCTAssertTrue(app.buttons["composer.retry"].waitForExistence(timeout: 5),
                      "The failed conversation offers Retry")
        attach("Conversation failed with every Endpoint request failing")
        hold(4)

        submit("go to Kilteevan Village")
        XCTAssertTrue(headerLabel(contains: "Kilteevan Village").waitForExistence(timeout: 8))
        submit("/exits")
        XCTAssertTrue(waitForText("Letter Office", timeout: 8))
        attach("Travel still works after the failed conversation")
        hold(5)
    }

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

/// Arrival descriptions and relaunch restore.
@MainActor
final class RundalePhase3ArrivalsUITests: RundalePhase3UITestCase {
    func testArrivalsListPeopleAndKeepRepeatVisitsBriefAfterRelaunch() {
        launch(reset: true)
        submit("go to Connolly Cottage")
        XCTAssertTrue(headerLabel(contains: "Connolly Cottage").waitForExistence(timeout: 8))
        let description = "A peat fire warms the single room."
        let firstDescription = rows(containing: description).firstMatch
        XCTAssertTrue(firstDescription.waitForExistence(timeout: 8))
        let originalDescriptionID = firstDescription.identifier
        let household = "A weathered man in a mud-spattered frieze coat and a young woman with yarn wound about her wrist are here."
        XCTAssertTrue(rows(containing: household).firstMatch.waitForExistence(timeout: 8))
        attach("First cottage visit describes the room and lists both people")

        // The opening described the village, where Peig now waits for the post.
        submit("go to Kilteevan Village")
        let villageReturn = rows(containing: "Scene. Kilteevan Village. A sharp-eyed woman with a satchel of letters is here.")
            .firstMatch
        XCTAssertTrue(villageReturn.waitForExistence(timeout: 8))
        XCTAssertFalse(villageReturn.label.contains("muddy road"), villageReturn.label)
        attach("Village return lists Peig without the description")
        app.terminate()
        app = XCUIApplication()
        launch(reset: false)
        XCTAssertTrue(headerLabel(contains: "Kilteevan Village").waitForExistence(timeout: 8))

        submit("go to Connolly Cottage")
        XCTAssertTrue(headerLabel(contains: "Connolly Cottage").waitForExistence(timeout: 8))
        let returnPresence = rows(containing: household).matching(NSPredicate(
            format: "identifier != %@", originalDescriptionID
        )).firstMatch
        XCTAssertTrue(returnPresence.waitForExistence(timeout: 8))
        XCTAssertFalse(returnPresence.label.contains("peat fire"), returnPresence.label)
        // Older rows may remain materialized. No newly appended description may appear.
        for row in rows(containing: description).allElementsBoundByIndex {
            XCTAssertEqual(row.identifier, originalDescriptionID)
        }
        attach("Returning after save resume lists people without repeating the description")

        submit("/look")
        XCTAssertTrue(rows(containing: "rain-dark coats by the door. Outside the weather is")
            .firstMatch.waitForExistence(timeout: 8))
        attach("Explicit look still describes a familiar room")
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
        XCTAssertTrue(waitForText("Smallholder and cattle drover", timeout: 8))
        XCTAssertTrue(waitForText("Spinner and household bookkeeper", timeout: 8))
    }
}

/// Clarification and tagged-person checks.
@MainActor
final class RundalePhase3ClarificationUITests: RundalePhase3UITestCase {
    func testAmbiguousConnollyRequiresSelectionBeforeEndpointWork() {
        launch(reset: true)
        submit("go to Connolly Cottage")
        submit("ask Connolly about the household")

        XCTAssertTrue(app.otherElements["clarification"].waitForExistence(timeout: 8))
        XCTAssertTrue(app.buttons["clarification.option.choose-npc-2"].waitForExistence(timeout: 3))
        let roisin = app.buttons["clarification.option.choose-npc-3"]
        XCTAssertTrue(roisin.waitForExistence(timeout: 3))
        let unresolvedPrompt = app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@", "Which Connolly do you mean?")
        ).firstMatch
        XCTAssertTrue(unresolvedPrompt.exists)
        XCTAssertFalse(app.buttons["composer.stop"].exists)
        roisin.tap()

        XCTAssertTrue(unresolvedPrompt.waitForNonExistence(timeout: 3))
        XCTAssertFalse(app.otherElements["clarification"].waitForExistence(timeout: 1))
        XCTAssertTrue(waitForText("household work allows", timeout: 15))
    }

    /// Spec Milestone 3: asking for someone who is elsewhere is answered
    /// "not here", never by whoever is present (#2047).
    func testAskingForSomeoneElsewhereSaysTheyAreNotHere() {
        launch(reset: true)
        XCTAssertTrue(headerLabel(contains: "Kilteevan Village").waitForExistence(timeout: 8))
        // 08:00: Peig waits on the village road; the Connollys are at home.
        submit("/wait 60")
        submit("/people")
        XCTAssertTrue(waitForText("Letter-office keeper", timeout: 8))
        hold(3)

        submit("ask Mícheál about the cattle")
        XCTAssertTrue(waitForText("Mícheál is not here.", timeout: 8))
        hold(2)
        submit("Mícheál, how are the cattle?")
        XCTAssertEqual(rows(containing: "Mícheál is not here.").count, 2)
        let dialogue = app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS 'Dialogue'"
        ))
        XCTAssertEqual(dialogue.count, 0, "Peig does not answer for Mícheál")
        XCTAssertFalse(app.buttons["composer.retry"].exists)
        attach("Asking for Mícheál in the village")
        hold(5)
    }

    func testTaggedNearbyPersonBypassesClarification() {
        launch(reset: true)
        submit("go to Connolly Cottage")

        let input = app.descendants(matching: .any)
            .matching(identifier: "composer.input")
            .firstMatch
        input.tap()
        input.typeText("Hello @")
        let micheal = app.buttons["completion.npc-2"]
        XCTAssertTrue(micheal.waitForExistence(timeout: 3))
        micheal.tap()
        XCTAssertEqual(input.value as? String, "Hello @a weathered man in a mud-spattered frieze coat")
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 3))
        app.buttons["composer.send"].tap()

        XCTAssertFalse(app.otherElements["clarification"].waitForExistence(timeout: 2))
        XCTAssertTrue(waitForText("moving cattle", timeout: 15))
    }
}
