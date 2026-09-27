import XCTest

/// Opt-in Release-style UI measurements. Results are attached as timing
/// evidence; the suite intentionally does not invent device-specific limits.
@MainActor
final class RundalePerformanceUITests: XCTestCase {
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

    func testOptInReleaseLaunchAndMemory() throws {
        try requireOptIn()
        let started = Date()
        measure(metrics: [XCTApplicationLaunchMetric(), XCTMemoryMetric(application: app)]) {
            app.launchArguments = ["--ui-tests", "--phase3", "--phase3-mock", "--no-auto-focus", "--reset-fixture"]
            app.launch()
            XCTAssertTrue(app.otherElements["status.header"].waitForExistence(timeout: 10))
        }
        attachTimingJSON(name: "launch-memory", started: started)
    }

    func testOptInLocalCommandTiming() throws {
        try requireOptIn()
        launch(reset: true)
        let options = XCTMeasureOptions()
        options.iterationCount = 3
        measure(metrics: [XCTClockMetric(), XCTMemoryMetric(application: app)], options: options) {
            commandInput.tap()
            commandInput.typeText("go east")
            app.buttons["composer.send"].tap()
            XCTAssertTrue(header(contains: "Letter Office").waitForExistence(timeout: 12))
            commandInput.tap()
            commandInput.typeText("go west")
            app.buttons["composer.send"].tap()
            XCTAssertTrue(header(contains: "Kilteevan Village").waitForExistence(timeout: 12))
        }
    }

    func testOptInTranscriptScrollTiming() throws {
        try requireOptIn()
        launch(reset: true, fixture: "long-history")
        let options = XCTMeasureOptions()
        options.iterationCount = 3
        measure(metrics: [XCTClockMetric(), XCTMemoryMetric(application: app)], options: options) {
            let transcript = app.collectionViews["transcript"]
            let before = visibleRows(in: transcript)
            XCTAssertFalse(before.isEmpty)
            transcript.swipeDown(velocity: .fast)
            let historical = visibleRows(in: transcript)
            XCTAssertFalse(historical.isEmpty)
            XCTAssertNotEqual(before, historical, "Scrolling must move the visible history")
            transcript.swipeUp(velocity: .fast)
            let newest = app.buttons["transcript.new-text"]
            if newest.exists { newest.tap() }
            XCTAssertNotEqual(visibleRows(in: transcript), historical)
        }
    }

    func testOptInStreamingTiming() throws {
        try requireOptIn()
        launch(reset: true)
        let started = Date()
        commandInput.tap()
        commandInput.typeText("ask Peig about the church slowly")
        app.buttons["composer.send"].tap()
        XCTAssertTrue(rows(containing: "The rain keeps").firstMatch.waitForExistence(timeout: 12))
        attachTimingJSON(name: "stream-first-chunk", started: started)
        XCTAssertTrue(rows(containing: "stands beyond the alder trees").matching(NSPredicate(
            format: "NOT label CONTAINS 'In progress'"
        )).firstMatch.waitForExistence(timeout: 25))
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 5))
        attachTimingJSON(name: "stream-final-commit", started: started)
    }

    private func visibleRows(in transcript: XCUIElement) -> Set<String> {
        Set(app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'transcript.item.'"
        )).allElementsBoundByIndex.filter {
            $0.isHittable && $0.frame.intersection(transcript.frame).height > 10
        }.map(\.identifier))
    }

    private func requireOptIn() throws {
        let environment = ProcessInfo.processInfo.environment
        let enabled = ProcessInfo.processInfo.arguments.contains("--performance")
            || environment["RUNDALE_PERFORMANCE_UI_TESTS"] == "1"
            || environment["TEST_RUNNER_RUNDALE_PERFORMANCE_UI_TESTS"] == "1"
        guard enabled else {
            throw XCTSkip("Set RUNDALE_PERFORMANCE_UI_TESTS=1 to run performance measurements")
        }
    }

    private func launch(reset: Bool, fixture: String? = nil) {
        app.launchArguments = fixture == nil
            ? ["--ui-tests", "--phase3", "--phase3-mock", "--no-auto-focus"]
            : ["--ui-tests", "--no-auto-focus"]
        if reset { app.launchArguments.append("--reset-fixture") }
        if let fixture { app.launchArguments.append("--fixture=\(fixture)") }
        app.launch()
        XCTAssertTrue(app.otherElements["status.header"].waitForExistence(timeout: 10))
        XCTAssertTrue(commandInput.waitForExistence(timeout: 5))
    }

    private var commandInput: XCUIElement {
        app.descendants(matching: .any).matching(identifier: "composer.input").firstMatch
    }

    private func header(contains text: String) -> XCUIElement {
        app.otherElements.matching(NSPredicate(
            format: "identifier == 'status.header' AND label CONTAINS %@", text
        )).firstMatch
    }

    private func rows(containing text: String) -> XCUIElementQuery {
        app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS %@", text
        ))
    }

    private func attachTimingJSON(name: String, started: Date, transport: String = "phase3-mock") {
        let object: [String: Any] = [
            "suite": "RundalePerformanceUITests",
            "case": name,
            "transport": transport,
            "harness_elapsed_seconds": Date().timeIntervalSince(started),
            "note": name == "launch-memory" ? "Total measurement-test wall time across repetitions; per-launch and memory samples are in xcresult metrics" : "One end-to-end XCTest UI timing sample, including harness and rendering overhead"
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: object, options: [.prettyPrinted, .sortedKeys]) else {
            return
        }
        let attachment = XCTAttachment(data: data, uniformTypeIdentifier: "public.json")
        attachment.name = "rundale-\(name)-timings.json"
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
