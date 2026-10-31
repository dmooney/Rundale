import XCTest

/// The completion strip scrolls sideways to the commands past the screen edge
/// (#2146). That is a gesture on the real tree, so it stays here; the rules it
/// offers (every command, each next word, names narrowing without fadas, the
/// short Commands list) and the commands' answers are tested in
/// `EngineScenarioTests`, and the engine's own answers on Linux
/// (limerick-mobile-ffi).
///
/// Set `TEST_RUNNER_RUNDALE_DEMO_HOLD=<seconds>` on `xcodebuild test` to hold
/// on each settled state when recording a demo (see the phase demo plan).
@MainActor
final class RundaleCommandsUITests: RundaleUITestCase {
    private var app: XCUIApplication!

    override func setUp() async throws {
        try await super.setUp()
        continueAfterFailure = false
        app = XCUIApplication()
        // No Endpoint is configured: a command that reached inference would
        // fail instead of answering.
        app.launchArguments = ["--ui-tests", "--phase2", "--no-auto-focus", "--reset-fixture"]
        app.launch()
        XCTAssertTrue(app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS %@",
            "A muddy road runs between low stone walls"
        )).firstMatch.waitForExistence(timeout: 15))
    }

    func testCompletionStripScrollsSidewaysToCommandsPastTheScreenEdge() {
        input.tap()
        input.typeText("/")
        for name in ["look", "help", "wait", "pause", "resume", "debug", "flags"] {
            XCTAssertTrue(app.buttons["completion.\(name)"].waitForExistence(timeout: 3), name)
        }
        hold(3)
        swipeStrip(until: "flags", direction: .left)
        hold(3)
        swipeStrip(until: "look", direction: .right)
        input.typeText("de")
        app.buttons["completion.debug"].tap()
        XCTAssertEqual(input.value as? String, "/debug ")
        XCTAssertTrue(app.buttons["completion.memory"].waitForExistence(timeout: 3))
        hold(3)
        swipeStrip(until: "help", direction: .left)
        hold(3)
        swipeStrip(until: "memory", direction: .right)
        app.buttons["completion.memory"].tap()
        XCTAssertEqual(input.value as? String, "/debug memory ")
        hold(3)
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
}
