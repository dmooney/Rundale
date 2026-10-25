import XCTest

/// Opt-in evidence against the deployed mobile data plane. The standard Phase
/// 2 verifier does not select this class; callers must provide the live origin
/// and private App Check debug-token environment explicitly.
@MainActor
final class RundaleLiveEndpointUITests: RundaleUITestCase {
    private var app: XCUIApplication!

    override func setUp() async throws {
        try await super.setUp()
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

    /// Stop while the dialogue call is open on the server (#2171). Gemini
    /// Flash Lite answers in under a second, and the tap itself takes a second
    /// to land, so one fixed delay would usually miss the dialogue call and
    /// stop the intent call or a finished reply. Step the delay across the
    /// window and record each attempt's wall-clock times: an attempt whose
    /// `/cancel` reaches the running dialogue stream is recorded by
    /// limerick-prod as a `rundale-dialogue` invocation with
    /// `error_code = REQUEST_CANCELLED` and `cancellation_requested = true`
    /// (request id `<request>:<attempt>.<call>`). Match the attached times to
    /// `invocations.started_at` with `endpoints/deploy/limerick-prod.sh sql`.
    /// `sent_at` precedes the Send tap's dispatch and `stopped_at` follows the
    /// Stop tap's return, so each is off by up to the tap time (about a second).
    /// The app never commits a reply for a turn it reports interrupted.
    func test09LiveStopAroundTheDialogueCallCommitsNothingLate() throws {
        try launch(reset: true)
        waitForInitialScene()
        goToTheCottage()

        var attempts: [[String: Any]] = []
        var stoppedCleanlyCount = 0
        let clock = ISO8601DateFormatter()
        clock.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        for step in 0..<4 {
            let delay = 0.2 + 0.15 * Double(step)
            let committedBefore = Set(app.committedDialogueRows().map(\.row))
            let interruptedBefore = interruptedRows.count
            let rowsBefore = Set(app.transcriptRows().map(\.row))
            let sentAt = submit("Mícheál, how are the cattle this week?")
            Thread.sleep(forTimeInterval: delay)
            let stop = app.buttons["composer.stop"]
            guard stop.exists else {
                attempts.append(["delay": delay, "outcome": "reply finished before Stop"])
                XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 20))
                continue
            }
            stop.tap()
            let stoppedAt = Date()
            XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
            RunLoop.current.run(until: Date().addingTimeInterval(3))
            let interrupted = interruptedRows.count > interruptedBefore
            let committed = !Set(app.committedDialogueRows().map(\.row)).subtracting(committedBefore).isEmpty
            let newRows = app.transcriptRows().filter { !rowsBefore.contains($0.row) }
            let retryOffered = app.buttons["composer.retry"].exists
            // A positive interruption: the app offers Retry for the stopped
            // request, the readable trace shows this turn's accepted command
            // and no failed row (a request failure also offers Retry), and no
            // reply committed. A Stop that did nothing commits the reply; a
            // failure leaves a failed row; an unreadable trace leaves no rows.
            let stoppedCleanly = retryOffered && !committed
                && newRows.contains { $0.kind == "player_command" }
                && !newRows.contains { $0.state.contains("fail") }
            if interrupted {
                XCTAssertFalse(committed, "A stopped turn must not commit a late reply")
            }
            if stoppedCleanly { stoppedCleanlyCount += 1 }
            attempts.append([
                "delay": delay,
                "sent_at": clock.string(from: sentAt),
                "stopped_at": clock.string(from: stoppedAt),
                "interrupted_row_shown": interrupted,
                "reply_committed": committed,
                "retry_offered": retryOffered,
                "new_rows": newRows.map { "\($0.kind)/\($0.state)" },
                "stopped_cleanly": stoppedCleanly
            ])
        }
        let trace = try JSONSerialization.data(
            withJSONObject: attempts, options: [.prettyPrinted, .sortedKeys]
        )
        let attachment = XCTAttachment(data: trace, uniformTypeIdentifier: "public.json")
        attachment.name = "rundale-live-stop-attempts.json"
        attachment.lifetime = .keepAlways
        add(attachment)
        XCTAssertGreaterThan(
            stoppedCleanlyCount, 0,
            "No attempt was positively interrupted by Stop: \(attempts)"
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

    /// Dynamic NPC context §8 row 25, the #1901 probe: asked where they are,
    /// the speaker answers from the snapshot's current location. Mícheál is
    /// at home in the cottage, so his accepted reply places him there and
    /// never at another place in the world.
    func test05LiveDialogueAgreesWithTheCurrentLocation() throws {
        try launch(reset: true)
        waitForInitialScene()
        goToTheCottage()
        _ = submit("Mícheál, where are we standing just now?")

        XCTAssertTrue(
            app.waitForTranscriptRow(timeout: 45) {
                $0.kind == "npc_dialogue" && $0.state == "committed"
            },
            "Expected an accepted live reply"
        )
        let reply = try XCTUnwrap(app.committedDialogueRows().last).text
        let attachment = XCTAttachment(string: reply)
        attachment.name = "rundale-live-location-probe-reply.txt"
        attachment.lifetime = .keepAlways
        add(attachment)

        // Fail on any other place in the world, and require words that put
        // him in the cottage: "here" alone would accept "here in the village".
        let lowered = reply.lowercased()
        for elsewhere in ["letter office", "post office", "kilteevan village",
                          "in the village", "at the village", "on the road", "in the lane"] {
            XCTAssertFalse(lowered.contains(elsewhere),
                           "Mícheál is in the cottage, not \(elsewhere): \(reply)")
        }
        XCTAssertTrue(
            ["cottage", "home", "house", "hearth", "kitchen"].contains { lowered.contains($0) },
            "The reply places Mícheál in the cottage: \(reply)"
        )
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
        hold(4)
    }

    /// Dynamic NPC context §8 row 2, live: "Hello" with both Connollys at
    /// home asks who it is for (each person, then All); choosing Róisín sends
    /// the line to her without retyping, through rundale-intent v1's decision
    /// and rundale-dialogue v1.
    func test06LiveUnaddressedHelloAsksWhoAndTheChoiceAnswers() throws {
        try launch(reset: true)
        waitForInitialScene()
        goToTheCottage()
        _ = submit("Hello")

        let clarification = app.otherElements["clarification"]
        XCTAssertTrue(clarification.waitForExistence(timeout: 45),
                      "Expected the game to ask who the line is for")
        XCTAssertTrue(waitForTranscriptText("Who are you speaking to?", timeout: 3))
        XCTAssertTrue(app.buttons["clarification.option.choose-all"].exists, "All is offered")
        XCTAssertFalse(dialogueRow(inProgress: false).exists, "No one answers before the choice")
        hold(4)

        app.buttons["clarification.option.choose-npc-3"].tap()
        XCTAssertTrue(clarification.waitForNonExistence(timeout: 8))
        XCTAssertTrue(
            app.waitForTranscriptRow(timeout: 45) { $0.kind == "npc_dialogue" && $0.state == "committed" },
            "Róisín answers the original line"
        )
        assertLastReplyIsRoisins()
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
        hold(5)
    }

    /// Dynamic NPC context §8 row 7, live: before any introduction, a line
    /// that names Róisín reaches her, not whoever is listed first.
    func test07LiveANameReachesItsPersonBeforeAnIntroduction() throws {
        try launch(reset: true)
        waitForInitialScene()
        goToTheCottage()
        _ = submit("Róisín, what news is there from the village?")

        XCTAssertTrue(
            app.waitForTranscriptRow(timeout: 45) { $0.kind == "npc_dialogue" && $0.state == "committed" },
            "Expected an accepted live reply"
        )
        XCTAssertFalse(app.otherElements["clarification"].exists, "A named line is not asked about")
        assertLastReplyIsRoisins()
        hold(5)
    }

    /// Dynamic NPC context §3.1, live, paced for a recording: with Peig alone
    /// on the village road an unaddressed line and a line naming Mícheál,
    /// who is at home, both reach her; at the cottage "Hello" asks who it is
    /// for; a line to Peig, who is on the road, reaches both Connollys.
    func test08LiveASpokenLineReachesThePeoplePresent() throws {
        try launch(reset: true)
        waitForInitialScene()
        hold(3)
        // 08:00: Peig waits on the village road; the Connollys are at home.
        _ = submit("/wait 60")
        _ = submit("/people")
        XCTAssertTrue(waitForTranscriptText("Letter-office keeper", timeout: 15))
        hold(3)

        _ = submit("Good morning to you")
        XCTAssertTrue(waitForCommittedReplies(1), "Peig answers")
        hold(6)
        _ = submit("Mícheál, are you about?")
        XCTAssertTrue(waitForCommittedReplies(2), "Peig answers a line naming Mícheál")
        XCTAssertFalse(app.transcriptRows().contains { $0.kind != "npc_dialogue" && $0.text.contains("not here") },
                       "No system line says anyone is not here")
        hold(6)

        goToTheCottage()
        hold(4)
        _ = submit("Hello")
        XCTAssertTrue(app.otherElements["clarification"].waitForExistence(timeout: 45))
        XCTAssertTrue(waitForTranscriptText("Who are you speaking to?", timeout: 3))
        hold(5)
        app.buttons["clarification.option.choose-npc-3"].tap()
        XCTAssertTrue(waitForCommittedReplies(3), "Róisín answers")
        hold(6)

        _ = submit("ask Peig about the road")
        XCTAssertTrue(waitForCommittedReplies(5, timeout: 90),
                      "both Connollys hear a line to Peig and answer: \(app.transcriptRows().suffix(6))")
        XCTAssertFalse(app.otherElements["clarification"].exists, "A line to someone elsewhere is not asked about")
        XCTAssertFalse(app.transcriptRows().contains { $0.kind != "npc_dialogue" && $0.text.contains("not here") },
                       "No system line says anyone is not here")
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 8))
        hold(8)
    }

    private func waitForCommittedReplies(_ count: Int, timeout: TimeInterval = 60) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            let seen = app.transcriptRows().filter { $0.kind == "npc_dialogue" && $0.state == "committed" }.count
            if seen >= count { return true }
            RunLoop.current.run(until: Date().addingTimeInterval(0.25))
        } while Date() < deadline
        return false
    }

    /// The last committed reply is Róisín's: her row is labelled with her
    /// description (or her name, once she has given it), never Mícheál's.
    private func assertLastReplyIsRoisins() {
        let replies = app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS 'Dialogue' AND NOT label CONTAINS 'In progress'"
        ))
        let label = replies.element(boundBy: replies.count - 1).label
        XCTAssertTrue(label.contains("yarn") || label.contains("Róisín"), "Róisín answers: \(label)")
        XCTAssertFalse(label.contains("frieze coat"), label)
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

    /// Rows the app marked "Interrupted; not applied", across all turns.
    private var interruptedRows: XCUIElementQuery {
        app.descendants(matching: .any).matching(
            NSPredicate(format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS 'Interrupted; not applied'")
        )
    }

    private func waitForTranscriptText(_ text: String, timeout: TimeInterval) -> Bool {
        app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@", text)
        ).firstMatch.waitForExistence(timeout: timeout)
    }
}
