import XCTest

/// Opt-in evidence against the deployed mobile data plane. The standard Phase
/// 2 verifier does not select this class; callers must provide the live origin
/// and private App Check debug-token environment explicitly.
final class RundaleLiveEndpointUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUp() {
        super.setUp()
        continueAfterFailure = false
        app = XCUIApplication()
    }

    func test01LiveEndpointStreamsAValidatedTerminalDialogue() throws {
        try launch(reset: true)
        waitForInitialScene()
        submit("ask Peig what she can tell me about this crossroads")

        let completed = dialogueRow(inProgress: false)
        XCTAssertTrue(completed.waitForExistence(timeout: 30), "Expected validated live dialogue")
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
        XCTAssertFalse(completed.label.contains("In progress"))
    }

    func test02LiveEndpointStopCancelsWithoutCommittingLateDialogue() throws {
        try launch(reset: true)
        waitForInitialScene()

        let input = commandInput
        XCTAssertTrue(input.waitForExistence(timeout: 5))
        input.tap()
        input.typeText("ask Peig for a careful answer about this crossroads")
        app.buttons["composer.send"].tap()

        let stop = app.buttons["composer.stop"]
        // The deployed Flash endpoint can finish in under a second. Query the
        // already-rendered control immediately so this test exercises the
        // in-flight cancellation path instead of racing the terminal frame.
        XCTAssertTrue(stop.exists || stop.waitForExistence(timeout: 0.2))
        Thread.sleep(forTimeInterval: 0.25)
        guard stop.exists else {
            throw XCTSkip("The live response completed before Stop could exercise cancellation")
        }
        stop.tap()

        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
        XCTAssertTrue(waitForTranscriptText("Interrupted; not applied", timeout: 8))
        XCTAssertFalse(dialogueRow(inProgress: false).exists)
    }

    private func launch(reset: Bool) throws {
        let environment = ProcessInfo.processInfo.environment
        let baseURL = environment["RUNDALE_LIVE_ENDPOINT_BASE_URL"]
            ?? "https://parish-server-24861210203.us-east1.run.app"
        app.launchArguments = ["--ui-tests", "--phase2", "--no-auto-focus"]
        if reset { app.launchArguments.append("--reset-fixture") }
        app.launchEnvironment["RUNDALE_ENDPOINT_BASE_URL"] = baseURL
        app.launchEnvironment["RUNDALE_ENDPOINT_ORGANIZATION"] =
            environment["RUNDALE_LIVE_ENDPOINT_ORGANIZATION"] ?? "parish-demo"
        app.launchEnvironment["RUNDALE_ENDPOINT_SLUG"] =
            environment["RUNDALE_LIVE_ENDPOINT_SLUG"] ?? "rundale-dialogue"
        app.launchEnvironment["RUNDALE_ENDPOINT_VERSION"] =
            environment["RUNDALE_LIVE_ENDPOINT_VERSION"] ?? "1"
        if let debugToken = environment["AppCheckDebugToken"], !debugToken.isEmpty {
            app.launchEnvironment["AppCheckDebugToken"] = debugToken
        }
        app.launch()
    }

    private func waitForInitialScene() {
        XCTAssertTrue(waitForTranscriptText("Rain darkens the road", timeout: 8))
        XCTAssertTrue(app.otherElements["status.header"].waitForExistence(timeout: 3))
    }

    private func submit(_ command: String) {
        let input = commandInput
        XCTAssertTrue(input.waitForExistence(timeout: 5))
        input.tap()
        input.typeText(command)
        app.buttons["composer.send"].tap()
        XCTAssertTrue(waitForTranscriptText(command, timeout: 8))
    }

    private var commandInput: XCUIElement {
        app.descendants(matching: .any)
            .matching(identifier: "composer.input")
            .firstMatch
    }

    private func dialogueRow(inProgress: Bool) -> XCUIElement {
        let progressClause = inProgress ? "AND label CONTAINS 'In progress'" : "AND NOT label CONTAINS 'In progress'"
        return app.descendants(matching: .any).matching(
            NSPredicate(
                format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS 'Dialogue' \(progressClause)"
            )
        ).firstMatch
    }

    private func waitForTranscriptText(_ text: String, timeout: TimeInterval) -> Bool {
        app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@", text)
        ).firstMatch.waitForExistence(timeout: timeout)
    }
}
