import XCTest

/// Product spec §5.4 on the shared engine (#2120): the advertised slash
/// commands are offered under Commands and answer locally, with no Endpoint
/// call; the unadvertised time and inspection commands also work. Typing
/// `/` completes every command step by step (#2146).
///
/// Set `TEST_RUNNER_RUNDALE_DEMO_HOLD=<seconds>` on `xcodebuild test` to hold
/// on each settled state when recording a demo (see the phase demo plan).
@MainActor
final class RundaleCommandsUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUp() async throws {
        try await super.setUp()
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

    /// #2146: typing `/` offers every command, and completion then offers
    /// each next word from the engine's registry; a name runs `/debug`
    /// locally with no Endpoint call.
    func testCompletionOffersEachNextWordAndRunsDebugMemory() {
        input.tap()
        input.typeText("/")
        for name in ["look", "help", "wait", "pause", "resume", "debug", "flags"] {
            XCTAssertTrue(app.buttons["completion.\(name)"].waitForExistence(timeout: 3), name)
        }
        hold(3)
        // The strip scrolls sideways to the commands past the screen edge.
        swipeStrip(until: "flags", direction: .left)
        hold(3)
        swipeStrip(until: "look", direction: .right)
        input.typeText("de")
        XCTAssertFalse(app.buttons["completion.look"].exists, "completion narrows as the player types")
        app.buttons["completion.debug"].tap()
        XCTAssertEqual(input.value as? String, "/debug ")
        XCTAssertTrue(app.buttons["completion.memory"].waitForExistence(timeout: 3))
        XCTAssertTrue(app.buttons["completion.clock"].exists)
        hold(3)
        swipeStrip(until: "help", direction: .left)
        hold(3)
        swipeStrip(until: "memory", direction: .right)
        app.buttons["completion.memory"].tap()
        XCTAssertEqual(input.value as? String, "/debug memory ")
        // Every NPC in the world, not only the ones on the road.
        for id in ["npc-1", "npc-2", "npc-3"] {
            XCTAssertTrue(app.buttons["completion.\(id)"].waitForExistence(timeout: 3), id)
        }
        hold(3)
        input.typeText("mic")
        XCTAssertFalse(app.buttons["completion.npc-1"].exists, "names narrow without fadas")
        app.buttons["completion.npc-2"].tap()
        XCTAssertEqual(input.value as? String, "/debug memory Mícheál Connolly ")
        hold(3)
        app.buttons["composer.send"].tap()
        XCTAssertTrue(row(containing: "[DEBUG MEMORY: Mícheál Connolly]").waitForExistence(timeout: 8))

        // Typed by hand without fadas, the engine still finds her.
        submit("/debug schedule roisin")
        XCTAssertTrue(row(containing: "[DEBUG SCHEDULE: Róisín Connolly]").waitForExistence(timeout: 8))
        XCTAssertFalse(app.buttons["composer.retry"].exists, "No command failed")
        hold(4)

        // The Commands button keeps the short list.
        app.buttons["composer.commands"].tap()
        XCTAssertTrue(app.buttons["completion.look"].waitForExistence(timeout: 3))
        XCTAssertFalse(app.buttons["completion.debug"].exists, "Commands lists only the advertised four")
    }

    /// Swipes the completion strip until `id`, off screen at first, is
    /// wholly on screen, then checks it can be tapped.
    private func swipeStrip(until id: String, direction: SwipeDirection) {
        let strip = app.scrollViews["composer.completions"]
        let target = app.buttons["completion.\(id)"]
        let screen = app.windows.firstMatch.frame
        XCTAssertTrue(target.waitForExistence(timeout: 3), id)
        XCTAssertFalse(screen.contains(target.frame), "\(id) starts off screen")
        for _ in 0..<4 where !screen.contains(target.frame) {
            switch direction {
            case .left: strip.swipeLeft(velocity: .slow)
            case .right: strip.swipeRight(velocity: .slow)
            }
        }
        XCTAssertTrue(screen.contains(target.frame), "a swipe brings \(id) on screen")
        XCTAssertTrue(target.isHittable, id)
    }

    private enum SwipeDirection { case left, right }

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
