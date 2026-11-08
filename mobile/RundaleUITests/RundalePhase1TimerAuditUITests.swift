import XCTest

/// The ordinary (non-test) fixture launch: its regions sit inside the screen
/// and a draft survives backgrounding. The scheduler's timed delivery is a
/// rule and is tested in `FixtureRuleTests` (RundaleSession).
@MainActor
final class RundalePhase1TimerAuditUITests: RundaleUITestCase {
    private var app: XCUIApplication!

    override func setUp() async throws {
        try await super.setUp()
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments = ["--fixture=long-history", "--reset-fixture", "--no-auto-focus"]
        app.launch()
    }

    private var input: XCUIElement { app.descendants(matching: .any).matching(identifier: "composer.input").firstMatch }
    private var scroll: XCUIElement { app.descendants(matching: .any).matching(identifier: "transcript").firstMatch }

    func testNonTestFixtureRegionsAndBackgroundDraft() {
        XCTAssertTrue(app.otherElements["status.header"].waitForExistence(timeout: 5))
        XCTAssertTrue(scroll.exists)
        XCTAssertTrue(app.otherElements["composer"].exists)
        let header = app.otherElements["status.header"]
        XCTAssertGreaterThanOrEqual(header.frame.minY, app.frame.minY)
        XCTAssertLessThanOrEqual(input.frame.maxY, app.frame.maxY - 10)
        XCTAssertGreaterThanOrEqual(input.frame.minX, app.frame.minX)
        XCTAssertLessThanOrEqual(input.frame.maxX, app.frame.maxX)
        let statusBar = app.statusBars.firstMatch
        if statusBar.exists {
            XCTAssertGreaterThanOrEqual(header.frame.minY, statusBar.frame.maxY)
        }
        XCTAssertFalse(app.tabBars.firstMatch.exists)
        for id in ["fixture.step", "composer.diagnostics", "Map", "Save", "NPC sidebar", "Portrait", "Scene art"] {
            XCTAssertFalse(app.buttons[id].exists, "Forbidden control: \(id)")
        }
        let screenshot = XCTAttachment(screenshot: app.screenshot())
        screenshot.name = "Non-test fixture screen; supports hierarchy inspection only"
        screenshot.lifetime = .keepAlways
        add(screenshot)
        input.tap()
        input.typeText("a substantial unfinished command for background restoration")
        let draft = input.value as? String
        XCTAssertEqual(draft, "a substantial unfinished command for background restoration")
        XCUIDevice.shared.press(.home)
        XCTAssertTrue(app.wait(for: .runningBackground, timeout: 5))
        app.activate()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 5))
        XCTAssertEqual(input.value as? String, draft)
    }
}
