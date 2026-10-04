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
        goToTheCottage()
        let sentAt = submit("Mícheál, how are the cattle this week?")

        // Free-form speech goes to rundale-intent v1, then rundale-dialogue v1.
        let completed = dialogueRow(inProgress: false)
        XCTAssertTrue(completed.waitForExistence(timeout: 45), "Expected validated live dialogue")
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
        XCTAssertFalse(completed.label.contains("In progress"))

        // A short live reply can stay provisional for ~100 ms, below the
        // accessibility poll interval, so read the app's record of every
        // dialogue-row state it published to the view.
        let rowID = String(completed.identifier.dropFirst("transcript.item.".count))
        let rowEntries = app.transcriptTrace().filter { $0.row == rowID }
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
            "note": "Includes tap injection, authentication, network, intent and dialogue model work, and UI polling; not isolated provider latency"
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
        goToTheCottage()
        _ = submit("Mícheál, how are the cattle this week?")

        // Stop while the turn is in flight: the intent call, then the
        // dialogue call, each take a moment against the deployed Endpoints.
        let stop = app.buttons["composer.stop"]
        guard stop.exists || stop.waitForExistence(timeout: 0.5) else {
            throw XCTSkip("The live turn completed before Stop could exercise cancellation")
        }
        let streamed = dialogueRow(inProgress: true).exists
        stop.tap()

        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
        if streamed {
            XCTAssertTrue(waitForTranscriptText("Interrupted; not applied", timeout: 8))
        }
        // A reply the Endpoint finishes after Stop is ignored by the engine.
        XCTAssertFalse(
            dialogueRow(inProgress: false).waitForExistence(timeout: 8),
            "A stopped turn must not commit a late reply"
        )
    }

    /// Free-form movement the local parser does not recognise is classified
    /// by rundale-intent v1; the engine then moves the player, once, and the
    /// transcript shows the move rather than a fallback conversation (#2046,
    /// carried from #1993).
    func test03LiveIntentEndpointClassifiesFreeFormMovement() throws {
        try launch(reset: true)
        waitForInitialScene()
        _ = submit("Let us be off, walking on toward the Letter Office")
        XCTAssertTrue(
            waitForTranscriptText("A narrow counter, pigeonholes of folded paper", timeout: 45),
            "Expected the Letter Office after the intent Endpoint classified the move"
        )
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
        XCTAssertTrue(app.otherElements.matching(NSPredicate(
            format: "identifier == 'status.header' AND label CONTAINS 'Letter Office'"
        )).firstMatch.exists)
        let walks = app.transcriptRows().filter {
            $0.text.contains("You walk along a short lane east to the whitewashed Letter Office")
        }
        XCTAssertEqual(walks.count, 1, "The move runs exactly once: \(walks)")
        XCTAssertTrue(app.transcriptRows().filter { $0.kind == "npc_dialogue" }.isEmpty,
                      "Unrecognised movement is not answered as dialogue")
        hold(4)
    }

    /// Product spec §5.3: rundale-intent v1 names "Connolly" as the
    /// addressee and both Connollys are at home, so the game asks which one
    /// before any dialogue call, then the choice continues the same request.
    func test04LiveIntentAddresseeSharedByTwoPeopleAsksWhichConnolly() throws {
        try launch(reset: true)
        waitForInitialScene()
        goToTheCottage()
        _ = submit("ask Connolly about the household")

        let clarification = app.otherElements["clarification"]
        XCTAssertTrue(clarification.waitForExistence(timeout: 45),
                      "Expected the game to ask which Connolly was meant")
        XCTAssertTrue(waitForTranscriptText("Which Connolly do you mean?", timeout: 3))
        let choices = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH 'clarification.option.'")
        )
        XCTAssertEqual(choices.count, 2, "One choice per Connolly at home")
        XCTAssertFalse(dialogueRow(inProgress: false).exists,
                       "No one answers before the player chooses")
        XCTAssertFalse(app.buttons["composer.stop"].exists,
                       "No dialogue call runs while the question is open")
        hold(4)

        choices.element(boundBy: 1).tap()
        XCTAssertTrue(clarification.waitForNonExistence(timeout: 8))
        XCTAssertTrue(dialogueRow(inProgress: false).waitForExistence(timeout: 45),
                      "The chosen Connolly answers the original question")
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
        hold(5)
    }

    /// Natural player speech is not an instruction to an invisible narrator.
    /// The engine asks who is being addressed before any NPC answers.
    func test05LiveNaturalSpinningQuestionContinuesWithChosenRoisin() throws {
        try launch(reset: true)
        waitForInitialScene()
        hold(3)
        goToTheCottage()
        hold(7)
        let speech = "Would you teach me how to spin, Miss?"
        _ = submit(speech)

        let clarification = app.otherElements["clarification"]
        XCTAssertTrue(clarification.waitForExistence(timeout: 8))
        XCTAssertFalse(dialogueRow(inProgress: false).exists,
                       "Nobody may answer before the recipient is selected")
        XCTAssertFalse(app.buttons["composer.stop"].exists,
                       "No model request should be open while choosing a recipient")
        let roisin = app.buttons["clarification.option.choose-npc-3"]
        XCTAssertTrue(roisin.waitForExistence(timeout: 3))
        hold(5)
        roisin.tap()

        XCTAssertTrue(clarification.waitForNonExistence(timeout: 8))
        let completed = dialogueRow(inProgress: false)
        XCTAssertTrue(completed.waitForExistence(timeout: 45))
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
        let commands = app.transcriptRows().filter {
            $0.kind == "player_command" && $0.text == speech
        }
        XCTAssertEqual(commands.count, 1, "The original question is submitted only once")
        XCTAssertTrue(completed.label.contains("young woman") || completed.label.contains("Róisín"),
                      "Róisín, rather than the cattle drover, must answer")
        hold(12)
    }

    /// Pauses for a human viewer when recording; a no-op in normal runs.
    private func hold(_ seconds: TimeInterval) {
        guard let value = ProcessInfo.processInfo.environment["RUNDALE_DEMO_HOLD"],
              let minimum = TimeInterval(value), minimum > 0 else { return }
        Thread.sleep(forTimeInterval: max(seconds, minimum))
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
        if let debugToken = environment["AppCheckDebugToken"], !debugToken.isEmpty {
            app.launchEnvironment["AppCheckDebugToken"] = debugToken
        }
        app.launch()
    }

    /// The engine's opening scene on the canonical world.
    private func waitForInitialScene() {
        XCTAssertTrue(waitForTranscriptText("A muddy road runs between low stone walls", timeout: 15))
        XCTAssertTrue(app.otherElements["status.header"].waitForExistence(timeout: 3))
    }

    /// Mícheál and Róisín are at home all morning. The local parser handles
    /// this, so no Endpoint call is made.
    private func goToTheCottage() {
        _ = submit("go to Connolly Cottage")
        XCTAssertTrue(waitForTranscriptText("A peat fire warms the single room", timeout: 15))
    }

    private func submit(_ command: String) -> Date {
        let input = commandInput
        XCTAssertTrue(input.waitForExistence(timeout: 5))
        input.tap()
        input.typeText(command)
        let sentAt = Date()
        app.buttons["composer.send"].tap()
        // Each test asserts what the command produced. The command row itself
        // can scroll out of the lazy transcript under a long reply.
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
