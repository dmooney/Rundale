import XCTest

/// Canonical Phase 3 checks against the embedded Rust world and native UI.
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

    func attach(_ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    /// Waits until exactly `count` committed replies contain `text`, read from
    /// the app's transcript trace so rows scrolled out of the lazy transcript
    /// still count.
    func waitForReplies(containing text: String, count: Int, timeout: TimeInterval) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        var seen = 0
        repeat {
            seen = app.transcriptRows().filter {
                $0.kind == "npc_dialogue" && $0.state == "committed" && $0.text.contains(text)
            }.count
            if seen >= count { break }
            RunLoop.current.run(until: Date().addingTimeInterval(0.25))
        } while Date() < deadline
        return seen == count
    }

    func waitForText(_ text: String, timeout: TimeInterval) -> Bool {
        app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@", text)
        ).firstMatch.waitForExistence(timeout: timeout)
    }
}

/// Place-name travel, and local play with every Endpoint request failing.
@MainActor
final class RundalePhase3UITests: RundalePhase3UITestCase {
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

    /// The app commits a reply as written: a reply naming a person who is not
    /// in the world (Tadhg Mulvihill appears in no mod file) still reaches the
    /// transcript verbatim, and survives a relaunch. The scripted reply comes
    /// from the mock Endpoint, not a model.
    func testReplyNamingAnUnknownPersonIsCommittedAsWritten() {
        launch(reset: true)
        XCTAssertTrue(headerLabel(contains: "Kilteevan Village").waitForExistence(timeout: 15))
        hold(3)
        submit("go to Connolly Cottage")
        XCTAssertTrue(headerLabel(contains: "Connolly Cottage").waitForExistence(timeout: 8))
        hold(3)
        submit("Mícheál, who is minding the cattle?")
        let reply = "Tadhg Mulvihill is minding them for me, and he has the grazing by the river."
        XCTAssertTrue(waitForReplies(containing: reply, count: 1, timeout: 30),
                      "the reply is committed unchanged")
        XCTAssertTrue(waitForText(reply, timeout: 5))
        attach("Reply naming an unknown person, shown as written")
        hold(6)

        app.terminate()
        launch(reset: false)
        XCTAssertTrue(waitForText(reply, timeout: 20), "the committed reply survives relaunch")
        attach("Same reply after relaunch")
        hold(6)
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

/// Clarification and tagged-person checks.
@MainActor
final class RundalePhase3ClarificationUITests: RundalePhase3UITestCase {
    func testAmbiguousConnollyRequiresSelectionBeforeEndpointWork() {
        launch(reset: true)
        submit("go to Connolly Cottage")
        submit("ask Connolly about the household")

        XCTAssertTrue(app.otherElements["clarification"].waitForExistence(timeout: 8))
        let micheal = app.buttons["clarification.option.choose-npc-2"]
        XCTAssertTrue(micheal.waitForExistence(timeout: 3))
        let roisin = app.buttons["clarification.option.choose-npc-3"]
        XCTAssertTrue(roisin.waitForExistence(timeout: 3))
        // Strangers are told apart by short labels (#2128).
        XCTAssertEqual(micheal.label, "the man")
        XCTAssertEqual(roisin.label, "the woman")
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

    /// Design §8 rows 1 to 3: "Hello" with both Connollys and no partner asks
    /// who it is for, listing each person and All (read as "Everyone here");
    /// choosing Róisín sends the line to her without retyping, and she
    /// becomes the partner, so the next unaddressed line goes to her.
    func testUnaddressedSpeechAsksWhoAndTheChoiceBecomesThePartner() {
        launch(reset: true)
        submit("go to Connolly Cottage")
        hold(2)
        submit("Hello")

        XCTAssertTrue(app.otherElements["clarification"].waitForExistence(timeout: 8))
        XCTAssertTrue(waitForText("Who are you speaking to?", timeout: 3))
        XCTAssertTrue(app.buttons["clarification.option.choose-npc-2"].exists)
        let all = app.buttons["clarification.option.choose-all"]
        XCTAssertTrue(all.exists)
        XCTAssertEqual(all.label, "Everyone here")
        XCTAssertFalse(app.buttons["composer.stop"].exists, "nothing is sent until the player answers")
        attach("Who are you speaking to?")
        hold(3)
        app.buttons["clarification.option.choose-npc-3"].tap()

        XCTAssertFalse(app.otherElements["clarification"].waitForExistence(timeout: 1))
        XCTAssertTrue(waitForText("household work allows", timeout: 15))
        hold(2)
        submit("And how's the harvest?")
        XCTAssertTrue(waitForReplies(containing: "household work allows", count: 2, timeout: 15),
                      "the partner answers")
        XCTAssertFalse(app.otherElements["clarification"].exists)
        hold(3)
    }
}
