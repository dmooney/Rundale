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
        XCTAssertTrue(completed.waitForExistence(timeout: 45), "Expected validated live dialogue")
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
        XCTAssertFalse(completed.label.contains("In progress"))

        // A short live reply can stay provisional for ~100 ms, below the
        // accessibility poll interval, so read the app's record of every
        // dialogue-row state it published to the view.
        let entries = app.transcriptTrace().filter { $0.kind == "npc_dialogue" }
        let rowID = try XCTUnwrap(
            entries.map(\.row).last { completed.identifier.contains($0) },
            "The completed row must appear in the transcript trace: \(entries)"
        )
        let rowEntries = entries.filter { $0.row == rowID }
        let provisionalIndex = try XCTUnwrap(
            rowEntries.firstIndex { $0.state == "provisional" && !$0.text.isEmpty },
            "Expected live Endpoint text in a provisional row before the terminal frame: \(rowEntries)"
        )
        let committedIndex = try XCTUnwrap(
            rowEntries.firstIndex { $0.state == "committed" },
            "The terminal event must finalize the same streamed row: \(rowEntries)"
        )
        XCTAssertLessThan(provisionalIndex, committedIndex)
        let trace = try JSONSerialization.data(
            withJSONObject: rowEntries.map {
                ["state": $0.state, "characters": $0.text.count, "milliseconds": $0.milliseconds]
            },
            options: [.prettyPrinted]
        )
        let traceAttachment = XCTAttachment(data: trace, uniformTypeIdentifier: "public.json")
        traceAttachment.name = "rundale-live-stream-trace.json"
        traceAttachment.lifetime = .keepAlways
        add(traceAttachment)
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
        XCTAssertTrue(app.committedDialogueRows().isEmpty)
        XCTAssertEqual(
            XCTWaiter.wait(for: [XCTestExpectation(description: "late live completion window")], timeout: 8),
            .timedOut
        )
        XCTAssertTrue(app.committedDialogueRows().isEmpty, "A delayed live completion must not commit after Stop")

        app.terminate()
        app = XCUIApplication()
        try launch(reset: false)
        waitForInitialScene()
        XCTAssertTrue(waitForTranscriptText("Interrupted; not applied", timeout: 8))
        XCTAssertTrue(app.committedDialogueRows().isEmpty, "Stopped state must remain uncommitted after relaunch")
    }

    /// Proves real Intent Endpoint execution by the native action it produces,
    /// not by a plausible NPC reply. The phrase is outside the local parser's
    /// recognised cases, so a location commit requires the Intent role.
    func test03LiveIntentEndpointExecutesTheInterpretedAction() throws {
        try launch(reset: true)
        waitForInitialScene()

        let header = app.otherElements["status.header"]
        XCTAssertTrue(header.label.localizedCaseInsensitiveContains("Kilteevan Village"))
        let sentAt = submit("Let's make for the Letter Office")

        XCTAssertTrue(
            app.waitForTranscriptRow(timeout: 30) { $0.text.contains("Travel to Letter Office.") },
            "Expected the live interpretation to select travel"
        )
        let moved = NSPredicate(format: "label CONTAINS[c] 'Letter Office'")
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [XCTNSPredicateExpectation(predicate: moved, object: header)],
                timeout: 30
            ),
            .completed,
            "Expected the interpreted action to change the authoritative location"
        )

        let correlation: [String: Any] = [
            "transport": "live-endpoint",
            "role": "intent",
            "player_input": "Let's make for the Letter Office",
            "selected_action": "travel",
            "resolved_target": "Letter Office",
            "send_to_committed_action_seconds": Date().timeIntervalSince(sentAt),
            "note": "Action correlation only; credentials and provider payloads are not recorded."
        ]
        let data = try JSONSerialization.data(
            withJSONObject: correlation,
            options: [.prettyPrinted, .sortedKeys]
        )
        let attachment = XCTAttachment(data: data, uniformTypeIdentifier: "public.json")
        attachment.name = "rundale-live-intent-correlation.json"
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    private func launch(reset: Bool) throws {
        let environment = ProcessInfo.processInfo.environment
        let baseURL = environment["RUNDALE_LIVE_ENDPOINT_BASE_URL"]
            ?? "https://limerick-endpoints-877612517009.us-east1.run.app"
        app.launchArguments = ["--ui-tests", "--phase2", "--no-auto-focus"]
        if reset { app.launchArguments.append("--reset-fixture") }
        app.launchEnvironment["RUNDALE_ENDPOINT_BASE_URL"] = baseURL
        app.launchEnvironment["RUNDALE_ENDPOINT_ORGANIZATION"] =
            environment["RUNDALE_LIVE_ENDPOINT_ORGANIZATION"] ?? "limerick-demo"
        app.launchEnvironment["RUNDALE_ENDPOINT_SLUG"] =
            environment["RUNDALE_LIVE_ENDPOINT_SLUG"] ?? "rundale-dialogue"
        app.launchEnvironment["RUNDALE_ENDPOINT_VERSION"] =
            environment["RUNDALE_LIVE_ENDPOINT_VERSION"] ?? "1"
        app.launchEnvironment["RUNDALE_INTENT_ENDPOINT_SLUG"] =
            environment["RUNDALE_LIVE_INTENT_ENDPOINT_SLUG"] ?? "rundale-intent"
        app.launchEnvironment["RUNDALE_INTENT_ENDPOINT_VERSION"] =
            environment["RUNDALE_LIVE_INTENT_ENDPOINT_VERSION"] ?? "1"
        if let debugToken = environment["AppCheckDebugToken"], !debugToken.isEmpty {
            app.launchEnvironment["AppCheckDebugToken"] = debugToken
        }
        app.launch()
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
        // The command row can scroll out of a small screen's viewport as
        // results arrive, so confirm acceptance from the published transcript.
        XCTAssertTrue(
            app.waitForTranscriptRow(timeout: 8) { $0.text.contains(command) },
            "\(command) must be accepted into the transcript"
        )
        return sentAt
    }

    private var commandInput: XCUIElement {
        app.descendants(matching: .any)
            .matching(identifier: "composer.input")
            .firstMatch
    }

    private func dialogueRow(inProgress: Bool) -> XCUIElement {
        let progressClause = inProgress
            ? "AND label CONTAINS 'In progress'"
            : "AND NOT label CONTAINS 'In progress' AND NOT label CONTAINS 'Interrupted; not applied'"
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
