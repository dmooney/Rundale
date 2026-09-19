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
        let sentAt = submit("ask Peig what she can tell me about this crossroads")

        let completed = dialogueRow(inProgress: false)
        XCTAssertTrue(completed.waitForExistence(timeout: 30), "Expected validated live dialogue")
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
        XCTAssertFalse(completed.label.contains("In progress"))
        let timing: [String: Any] = [
            "transport": "live-endpoint",
            "send_to_final_ui_seconds": Date().timeIntervalSince(sentAt),
            "note": "Includes tap injection, authentication, network, model work, and UI polling; not isolated provider latency"
        ]
        let data = try JSONSerialization.data(withJSONObject: timing, options: [.prettyPrinted, .sortedKeys])
        let attachment = XCTAttachment(data: data, uniformTypeIdentifier: "public.json")
        attachment.name = "rundale-live-final-timing.json"
        attachment.lifetime = .keepAlways
        add(attachment)
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
        guard stop.exists || stop.waitForExistence(timeout: 0.2) else {
            throw XCTSkip("The live response completed before Stop could exercise cancellation")
        }
        stop.tap()

        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
        XCTAssertTrue(waitForTranscriptText("Interrupted; not applied", timeout: 8))
        XCTAssertFalse(dialogueRow(inProgress: false).exists)
    }

    /// Opt-in #1993 live Intent gate. Requires Firebase App Check + published
    /// `rundale-intent` (or `RUNDALE_LIVE_ENDPOINT_INTENT_SLUG`). Skips when the
    /// debug token is absent so standard CI does not claim a pass.
    func test03LiveIntentEndpointMovesToAuthoredDestination() throws {
        let environment = ProcessInfo.processInfo.environment
        guard let debugToken = environment["AppCheckDebugToken"], !debugToken.isEmpty else {
            throw XCTSkip("Live Intent Endpoint blocked: AppCheckDebugToken unavailable")
        }
        try launchIntent(reset: true, debugToken: debugToken)
        waitForInitialScene()
        XCTAssertTrue(headerLabel(contains: "Kilteevan Village").waitForExistence(timeout: 8))
        _ = submit("take me to the Letter Office")
        XCTAssertTrue(headerLabel(contains: "Letter Office").waitForExistence(timeout: 45))
        XCTAssertTrue(waitForTranscriptText("Travel to Letter Office.", timeout: 15))
        XCTAssertFalse(headerLabel(contains: "Kilteevan Village").exists)
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

    private func launchIntent(reset: Bool, debugToken: String) throws {
        let environment = ProcessInfo.processInfo.environment
        let baseURL = environment["RUNDALE_LIVE_ENDPOINT_BASE_URL"]
            ?? "https://parish-server-24861210203.us-east1.run.app"
        // Phase 3 world so Letter Office exists; live transport (no --phase3-mock).
        app.launchArguments = ["--ui-tests", "--phase3", "--no-auto-focus"]
        if reset { app.launchArguments.append("--reset-fixture") }
        app.launchEnvironment["RUNDALE_ENDPOINT_BASE_URL"] = baseURL
        app.launchEnvironment["RUNDALE_ENDPOINT_ORGANIZATION"] =
            environment["RUNDALE_LIVE_ENDPOINT_ORGANIZATION"] ?? "parish-demo"
        app.launchEnvironment["RUNDALE_ENDPOINT_SLUG"] =
            environment["RUNDALE_LIVE_ENDPOINT_SLUG"] ?? "rundale-dialogue"
        app.launchEnvironment["RUNDALE_ENDPOINT_INTENT_SLUG"] =
            environment["RUNDALE_LIVE_ENDPOINT_INTENT_SLUG"] ?? "rundale-intent"
        app.launchEnvironment["RUNDALE_ENDPOINT_VERSION"] =
            environment["RUNDALE_LIVE_ENDPOINT_VERSION"] ?? "1"
        app.launchEnvironment["AppCheckDebugToken"] = debugToken
        app.launch()
    }

    private func headerLabel(contains text: String) -> XCUIElement {
        app.otherElements.matching(
            NSPredicate(format: "identifier == 'status.header' AND label CONTAINS[c] %@", text)
        ).firstMatch
    }

    private func waitForInitialScene() {
        XCTAssertTrue(waitForTranscriptText("Morning gathers over Kilteevan", timeout: 8))
        XCTAssertTrue(app.otherElements["status.header"].waitForExistence(timeout: 3))
    }

    private func submit(_ command: String) -> Date {
        let input = commandInput
        XCTAssertTrue(input.waitForExistence(timeout: 5))
        input.tap()
        input.typeText(command)
        let sentAt = Date()
        app.buttons["composer.send"].tap()
        XCTAssertTrue(waitForTranscriptText(command, timeout: 8))
        return sentAt
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
