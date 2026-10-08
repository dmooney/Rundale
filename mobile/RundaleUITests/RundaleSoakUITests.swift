import XCTest

/// Opt-in endurance coverage for a real local world session. The default run
/// is twenty minutes; RUNDALE_SOAK_SMOKE=1 selects a short smoke run.
@MainActor
final class RundaleSoakUITests: XCTestCase {
    private var app: XCUIApplication!

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

    func testOptInLocalWorldSoak() throws {
        let environment = ProcessInfo.processInfo.environment
        guard optIn(environment: environment, variable: "RUNDALE_SOAK_UI_TESTS", flag: "--soak") else {
            throw XCTSkip("Set RUNDALE_SOAK_UI_TESTS=1 to run the 20-minute local-world soak")
        }
        let smoke = environmentValue(environment, "RUNDALE_SOAK_SMOKE") == "1"
        let defaultDuration: TimeInterval = smoke ? 30 : 20 * 60
        let duration = max(5, TimeInterval(environmentValue(environment, "RUNDALE_SOAK_DURATION_SECONDS") ?? "") ?? defaultDuration)
        launch(reset: true)
        XCTAssertTrue(header(contains: "Kilteevan Village").waitForExistence(timeout: 8))

        submit("go to Connolly Cottage")
        XCTAssertTrue(header(contains: "Connolly Cottage").waitForExistence(timeout: 8))
        submit("go to the Letter Office")
        XCTAssertTrue(header(contains: "Letter Office").waitForExistence(timeout: 8))

        let started = Date()
        var travelCount = 0
        var streamCount = 0
        var backgroundCount = 0
        var cycle = 0
        while Date().timeIntervalSince(started) < duration {
            submit("go to Kilteevan Village")
            XCTAssertTrue(header(contains: "Kilteevan Village").waitForExistence(timeout: 8))
            submit("go to the Letter Office")
            XCTAssertTrue(header(contains: "Letter Office").waitForExistence(timeout: 8))
            travelCount += 1
            let replyRow = askPeigOnceSheIsAtTheCounter()
            XCTAssertTrue(app.waitForTranscriptRow(timeout: 25) {
                $0.row == replyRow && $0.state == "committed"
            }, "This cycle's provisional row must become a committed reply")
            // Until she introduces herself, Peig is named by her description.
            let final = app.descendants(matching: .any).matching(NSPredicate(
                format: "identifier == %@ AND label CONTAINS 'Dialogue' AND (label CONTAINS 'Peig Hannigan' OR label CONTAINS 'satchel of letters') AND NOT label CONTAINS 'In progress' AND NOT label CONTAINS 'not applied'",
                "transcript.item.\(replyRow)"
            )).firstMatch
            XCTAssertTrue(final.waitForExistence(timeout: 5), "The committed reply must be on screen")
            XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 5))
            XCTAssertFalse(app.buttons["composer.retry"].exists)
            streamCount += 1

            if cycle.isMultiple(of: 3) {
                let transcript = app.collectionViews["transcript"]
                let newestIdentifier = final.identifier
                let before = visibleRows(in: transcript)
                transcript.swipeDown()
                let historical = visibleRows(in: transcript)
                XCTAssertFalse(historical.isEmpty)
                XCTAssertNotEqual(before, historical, "Scrolling must expose different historical rows")
                let jump = app.buttons["transcript.new-text"]
                if jump.exists { jump.tap() } else { transcript.swipeUp() }
                let newest = app.descendants(matching: .any).matching(identifier: newestIdentifier).firstMatch
                XCTAssertTrue(newest.waitForExistence(timeout: 5))
                XCTAssertTrue(newest.isHittable, "Returning to newest must reveal this cycle's reply")
                XCUIDevice.shared.press(.home)
                XCTAssertTrue(app.wait(for: .runningBackground, timeout: 5))
                app.activate()
                XCTAssertTrue(app.wait(for: .runningForeground, timeout: 5))
                XCTAssertTrue(header(contains: "Letter Office").waitForExistence(timeout: 8))
                backgroundCount += 1
            }
            cycle += 1
        }

        XCTAssertGreaterThan(travelCount, 0)
        XCTAssertGreaterThan(streamCount, 0)
        XCTAssertGreaterThanOrEqual(backgroundCount, 1)
        attachTimingJSON([
            "suite": "RundaleSoakUITests",
            "transport": "phase3-mock",
            "smoke": smoke,
            "duration_seconds": Date().timeIntervalSince(started),
            "travel_cycles": travelCount,
            "stream_cycles": streamCount,
            "background_recoveries": backgroundCount
        ])
    }

    /// Peig waits on the village road from 07:00 to 08:59, so a long run's
    /// game clock reaches a morning when she is away from the Letter Office.
    /// The engine then answers "Peig is not here." without a model call;
    /// waiting at the counter (`/wait`, the local command) until she returns
    /// keeps every cycle a dialogue.
    /// Returns the transcript row of Peig's reply.
    private func askPeigOnceSheIsAtTheCounter() -> String {
        for _ in 0 ..< 12 {
            let earlier = Set(app.transcriptRows().map(\.row))
            submit("ask Peig about the church slowly")
            var answer: TranscriptTraceEntry?
            XCTAssertTrue(app.waitForTranscriptRow(timeout: 12) { entry in
                guard !earlier.contains(entry.row),
                      entry.kind == "npc_dialogue" || entry.text.contains("Peig is not here")
                else { return false }
                answer = entry
                return true
            }, "Asking Peig must produce her reply or say she is away")
            guard let answer else { break }
            if answer.kind == "npc_dialogue" { return answer.row }
            submit("/wait 30")
        }
        XCTFail("Peig never returned to the Letter Office")
        return ""
    }

    private func visibleRows(in transcript: XCUIElement) -> Set<String> {
        Set(app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'transcript.item.'"
        )).allElementsBoundByIndex.filter {
            $0.isHittable && $0.frame.intersection(transcript.frame).height > 10
        }.map(\.identifier))
    }

    private func optIn(environment: [String: String], variable: String, flag: String) -> Bool {
        ProcessInfo.processInfo.arguments.contains(flag) || environmentValue(environment, variable) == "1"
    }

    private func environmentValue(_ environment: [String: String], _ key: String) -> String? {
        environment[key] ?? environment["TEST_RUNNER_\(key)"]
    }

    private func launch(reset: Bool) {
        app.launchArguments = ["--ui-tests", "--phase3", "--phase3-mock", "--no-auto-focus"]
        if reset { app.launchArguments.append("--reset-fixture") }
        app.launch()
        XCTAssertTrue(app.otherElements["status.header"].waitForExistence(timeout: 10))
        XCTAssertTrue(commandInput.waitForExistence(timeout: 5))
    }

    private func submit(_ command: String) {
        commandInput.tap()
        commandInput.typeText(command)
        app.buttons["composer.send"].tap()
        let accepted = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == '' OR value == nil"), object: commandInput)
        XCTAssertEqual(XCTWaiter.wait(for: [accepted], timeout: 8), .completed)
        if app.buttons["transcript.new-text"].exists {
            app.buttons["transcript.new-text"].tap()
        }
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

    private func attachTimingJSON(_ object: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: object, options: [.prettyPrinted, .sortedKeys]) else {
            return
        }
        let attachment = XCTAttachment(data: data, uniformTypeIdentifier: "public.json")
        attachment.name = "rundale-soak-timings.json"
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
