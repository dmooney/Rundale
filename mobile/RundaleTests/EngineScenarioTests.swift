import Foundation
import LimerickEndpointKit
import RundaleKit
import RundaleSession
import XCTest
@testable import Rundale

/// The embedded-engine scenarios the phase UI suites used to drive through the
/// accessibility tree (#2319): the real Rust engine and SQLite save, the
/// bundled canonical world, the scripted mock Endpoint, and the real presenter,
/// in the app-hosted test process. Only the view and the process lifecycle are
/// left out; a relaunch here is a new controller over the same save, as in
/// `RundalePhase4HistoryTests`. Each test names the XCUITest it replaced.
@MainActor
final class EngineScenarioTests: XCTestCase {
    private static let opening = "A muddy road runs between low stone walls"
    private var directory: URL!
    private var game: Game!

    override func setUp() async throws {
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("EngineScenarioTests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        game = Game(directory: directory)
    }

    override func tearDown() async throws {
        game = nil
        try? FileManager.default.removeItem(at: directory)
    }

    // MARK: Commands

    /// Was `RundaleCommandsUITests.testNewGameStartsFreshOfflineAndResumesAfterRelaunch`.
    func testNewGameStartsFreshOfflineAndResumesAfterRelaunch() async throws {
        try await game.launch()
        await game.submit("/wait 60")
        await game.waitForRow("You wait for 60 minutes")
        game.type("/ne")
        XCTAssertTrue(game.presenter.completions.contains { $0.id == "new" })
        game.select("new")
        game.presenter.submitDraft()
        await waitUntil { game.presenter.draft.isEmpty && !game.hasRow("You wait for 60 minutes") }
        XCTAssertTrue(game.hasRow(Self.opening), "the new game opens on the opening scene")
        await game.submit("/wait 7")
        await game.waitForRow("You wait for 7 minutes")

        try await game.relaunch()
        await game.waitForRow("You wait for 7 minutes")
        XCTAssertFalse(game.hasRow("You wait for 60 minutes"))
        await game.submit("/NEW-GAME", makesARow: false)
        await waitUntil { !game.hasRow("You wait for 7 minutes") }
        XCTAssertTrue(game.hasRow(Self.opening))
        await game.submit("/help")
        await game.waitForRow("/new")
    }

    /// Was `RundaleCommandsUITests.testUnadvertisedCommandsWorkAndOthersAreRefused`.
    func testUnadvertisedCommandsWorkAndOthersAreRefused() async throws {
        try await game.launch()
        await game.submit("/wait 60")
        await game.waitForRow("You wait for 60 minutes")
        await game.submit("/debug clock")
        await game.waitForRow("[DEBUG CLOCK]")

        game.type("/save")
        game.presenter.submitDraft()
        await waitUntil { game.presenter.submissionMessage != nil || game.hasRow("That command isn't available here") }
        XCTAssertTrue(
            game.presenter.submissionMessage?.contains("That command isn't available here. Try /help.") == true
                || game.hasRow("That command isn't available here. Try /help."),
            "\(game.presenter.submissionMessage ?? "no message"); \(game.labels.suffix(2))"
        )
        XCTAssertEqual(game.presenter.draft, "/save", "A refused command keeps the draft")
    }

    /// Was `RundaleCommandsUITests.testCompletionOffersEachNextWordAndRunsDebugMemory`
    /// (its sideways swipes of the strip stay in `RundaleCommandsUITests`).
    func testCompletionOffersEachNextWordAndRunsDebugMemory() async throws {
        try await game.launch()
        game.type("/")
        let all = game.completionIDs
        for name in ["look", "help", "wait", "pause", "resume", "debug", "flags"] {
            XCTAssertTrue(all.contains(name), name)
        }
        game.type("/de")
        XCTAssertFalse(game.completionIDs.contains("look"), "completion narrows as the player types")
        game.select("debug")
        XCTAssertEqual(game.presenter.draft, "/debug ")
        XCTAssertTrue(game.completionIDs.contains("memory"))
        XCTAssertTrue(game.completionIDs.contains("clock"))
        game.select("memory")
        XCTAssertEqual(game.presenter.draft, "/debug memory ")
        for id in ["npc-1", "npc-2", "npc-3"] {
            XCTAssertTrue(game.completionIDs.contains(id), "everyone in the world, not only the road: \(id)")
        }
        game.type("/debug memory mic")
        XCTAssertFalse(game.completionIDs.contains("npc-1"), "names narrow without fadas")
        game.select("npc-2")
        XCTAssertEqual(game.presenter.draft, "/debug memory Mícheál Connolly ")
        game.presenter.submitDraft()
        await game.waitForRow("[DEBUG MEMORY: Mícheál Connolly]")

        await game.submit("/debug schedule roisin")
        await game.waitForRow("[DEBUG SCHEDULE: Róisín Connolly]")
        XCTAssertFalse(game.presenter.showsRetry, "No command failed")

        game.presenter.browseCompletions("/")
        XCTAssertTrue(game.completionIDs.contains("look"))
        XCTAssertFalse(game.completionIDs.contains("debug"), "Commands lists only the advertised four")
    }

    /// Was `RundaleCommandsUITests.testBugCommandSendsAReportWithoutATurn`.
    func testBugCommandSendsAReportWithoutATurn() async throws {
        try await game.launch(bundle: ["RUNDALE_BETA_FEEDBACK": "YES"], screenshot: Data([1, 2, 3]))
        await game.submit("go to Connolly Cottage")
        await game.waitForRow("A peat fire warms the single room")
        game.type("/bu")
        XCTAssertTrue(game.completionIDs.contains("bug"))
        game.select("bug")
        XCTAssertEqual(game.presenter.draft, "/bug ")
        game.type("/bug Mícheál never looked up")
        game.presenter.submitDraft()
        await waitUntil { game.presenter.bugReportNotice == RundaleSessionPresenter.bugReportSentNotice }
        let text = game.presenter.uiTestBugReport
        XCTAssertTrue(text.hasPrefix("screenshot bytes: "), text)
        XCTAssertFalse(text.hasPrefix("screenshot bytes: 0\n"), "a screenshot was taken")
        XCTAssertTrue(text.contains("Rundale bug report\nMícheál never looked up\n"), text)
        XCTAssertTrue(text.contains("Scene: Connolly Cottage · "), text)
        XCTAssertTrue(text.contains("> go to Connolly Cottage\n"), text)
        await waitUntil { game.presenter.draft.isEmpty }
        XCTAssertFalse(game.presenter.transcript.contains { $0.text.contains("/bug") }, "/bug is not a turn")
        XCTAssertFalse(game.presenter.showsRetry)
        game.presenter.dismissBugReportNotice()
        XCTAssertNil(game.presenter.bugReportNotice)
    }

    // MARK: Scenes

    /// Was `RundaleSceneUITests.testOpeningAndArrivalAreTitledSceneCards`.
    func testOpeningAndArrivalAreTitledSceneCards() async throws {
        try await game.launch()
        await game.waitForRow(Self.opening)
        let opening = try XCTUnwrap(game.sceneLabel("Kilteevan Village", "A muddy road runs between low stone walls"))
        game.assertNoRestatedHeader(opening)

        await game.submit("go to Connolly Cottage")
        await waitUntil { game.sceneLabel("Connolly Cottage", "A peat fire warms the single room") != nil }
        let arrival = try XCTUnwrap(game.sceneLabel("Connolly Cottage", "A peat fire warms the single room"))
        game.assertNoRestatedHeader(arrival)
        XCTAssertTrue(arrival.hasSuffix("are here."), arrival)
        XCTAssertFalse(game.hasRow("You can go to"), "The transcript lists no exits on arrival")
    }

    /// Was `RundaleSceneUITests.testReturningShowsWhoIsThereWithoutRepeatingTheDescription`.
    func testReturningShowsWhoIsThereWithoutRepeatingTheDescription() async throws {
        try await game.launch()
        await game.waitForRow("A muddy road")
        await game.submit("go to Connolly Cottage")
        await waitUntil { game.sceneLabel("Connolly Cottage", "A peat fire warms the single room") != nil }

        await game.submit("go to Kilteevan Village")
        await waitUntil {
            game.labels.contains { $0.hasPrefix("Scene. Kilteevan Village. ") && $0.hasSuffix("is here.") }
        }
        let villageReturn = game.labels.last { $0.hasPrefix("Scene. Kilteevan Village. ") }
        XCTAssertFalse(villageReturn?.contains("muddy road") ?? true, villageReturn ?? "")

        await game.submit("go to the Letter Office")
        await waitUntil { game.sceneLabel("Letter Office", "A narrow counter") != nil }
        await game.submit("go to Kilteevan Village")
        await game.submit("go to the Letter Office")
        await waitUntil { game.labels.contains("Scene. Letter Office") }

        await game.submit("look")
        await game.waitForRow("A narrow counter, pigeonholes")
    }

    // MARK: Phase 2

    /// Was `RundalePhase2UITests.testPhase2LookBatchClearsComposerAfterRustAcceptance`.
    func testLookClearsTheComposerAfterTheEngineAcceptsIt() async throws {
        try await game.launch()
        await game.waitForRow(Self.opening)
        XCTAssertTrue(game.presenter.header.location.contains("Kilteevan Village"))
        game.type("/look")
        game.presenter.submitDraft()
        await game.waitForRow("It is morning.")
        await waitUntil { game.presenter.draft.isEmpty && !game.presenter.isStreaming }
        await game.submit("/wait 10")
        await game.submit("/people")
        await game.waitForRow("a satchel of letters")
    }

    /// Was `RundalePhase2UITests.testPhase2CompletionsUseRustNearbyPeople`.
    func testCompletionsUseTheEnginesNearbyPeople() async throws {
        try await game.launch()
        await game.goToTheCottage()
        game.type("ask @")
        let ids = game.completionIDs
        XCTAssertTrue(ids.contains("npc-2"), "Mícheál is at home")
        XCTAssertTrue(ids.contains("npc-3"), "Róisín is at home")
        XCTAssertFalse(ids.contains("npc-1"), "Peig is not here")
    }

    /// Was `RundalePhase2DialogueUITests.testPhase2IncrementalChunksUpdateOneDialogueRowBeforeFinal`.
    func testIncrementalChunksUpdateOneDialogueRowBeforeFinal() async throws {
        try await game.launch()
        await game.goToTheCottage()
        await game.submit("ask Mícheál about the cattle")
        await waitUntil { game.dialogue(containing: "The wet ground") != nil }
        let provisional = try XCTUnwrap(game.dialogue(containing: "The wet ground"))
        XCTAssertTrue(provisional.accessibilityLabel.contains("In progress"))
        await waitUntil { game.dialogue(containing: "has made moving cattle") != nil }
        await waitUntil { game.dialogue(containing: "difficult this week") != nil }
        await waitUntil { game.presenter.transcript.first { $0.id == provisional.id }?.state == .committed }
        let completed = try XCTUnwrap(game.presenter.transcript.first { $0.id == provisional.id })
        XCTAssertFalse(completed.accessibilityLabel.contains("In progress"))
        XCTAssertTrue(completed.text.contains("difficult this week"), "the same row holds the final reply")
    }

    /// Was `RundalePhase2DialogueUITests.testPhase2EndpointErrorNamesTheFailureCategory`.
    func testEndpointErrorShowsTheModsLineForItsCategory() async throws {
        try await game.launch()
        await game.goToTheCottage()
        await game.submit("ask Mícheál to fail")
        await game.waitForRow("The storyteller has gone out to the bog and isn't back yet. Try again in a while.")
        XCTAssertFalse(game.hasRow("could not be completed"))
    }

    /// Was `RundalePhase2RecoveryUITests.testPhase2StopLeavesInterruptedAttemptAndRetryCompletes`.
    func testStopLeavesAnInterruptedAttemptAndRetryCompletes() async throws {
        try await game.launch()
        await game.goToTheCottage()
        await game.submit("ask Mícheál about the cattle slowly")
        await waitUntil { game.dialogue(containing: "The wet ground") != nil }
        game.presenter.stop()
        await waitUntil { !game.presenter.isStreaming && game.presenter.showsRetry }
        XCTAssertTrue(game.hasRow("Interrupted; not applied"))

        game.presenter.retryLastFailed()
        await waitUntil { game.dialogue(containing: "The wet ground has made moving cattle") != nil }
        await waitUntil(timeout: 60) { game.dialogue(containing: "difficult this week") != nil }
        await waitUntil { !game.presenter.isStreaming }
        XCTAssertTrue(game.hasRow("Interrupted; not applied"), "the stopped attempt stays in the transcript")
        XCTAssertFalse(game.presenter.showsRetry)
    }

    /// Was `RundalePhase2RecoveryUITests.testPhase2RetryIsGoneAfterRecoveryAndAfterTheNextQuestion` (#2329).
    func testRetryIsGoneAfterRecoveryAndAfterTheNextQuestion() async throws {
        try await game.launch()
        await game.goToTheCottage()
        await game.submit("ask Mícheál about the cattle slowly")
        await waitUntil { game.presenter.showsStop }
        game.presenter.stop()
        await waitUntil { game.presenter.showsRetry }
        game.presenter.retryLastFailed()
        await waitUntil(timeout: 60) { game.dialogue(containing: "difficult this week") != nil }
        await waitUntil { !game.presenter.isStreaming }
        XCTAssertFalse(game.presenter.showsRetry, "A recovered reply leaves nothing to retry")
        XCTAssertFalse(game.hasRow("could not be completed"))

        await game.submit("ask Mícheál about the weather slowly")
        await waitUntil { game.presenter.showsStop }
        game.presenter.stop()
        await waitUntil { game.presenter.showsRetry }
        await game.submit("ask Mícheál about the cattle")
        await waitUntil { game.presenter.showsStop }
        await waitUntil(timeout: 60) { !game.presenter.isStreaming }
        XCTAssertFalse(game.presenter.showsRetry,
                       "Retry must not outlive the newer question that replaced the stopped one")
    }

    /// Was `RundalePhase2RecoveryUITests.testPhase2SQLiteRestoresCommittedDialogueAfterRelaunch`.
    func testSQLiteRestoresCommittedDialogueAfterRelaunch() async throws {
        try await game.launch()
        await game.goToTheCottage()
        await game.submit("ask Mícheál about the cattle")
        await waitUntil(timeout: 60) { game.dialogue(containing: "difficult this week") != nil }
        await waitUntil { !game.presenter.isStreaming }

        try await game.relaunch()
        await waitUntil {
            game.presenter.transcript.contains { $0.kind == .playerCommand && $0.text.contains("ask Mícheál about the cattle") }
        }
        XCTAssertNotNil(game.dialogue(containing: "The wet ground has made moving cattle"))
        XCTAssertFalse(game.presenter.isStreaming)
    }

    // MARK: Phase 3

    /// Was `RundalePhase3UITests.testLocalCommandsAndTravelWorkWhenEveryEndpointRequestFails`.
    func testLocalCommandsAndTravelWorkWhenEveryEndpointRequestFails() async throws {
        try await game.launch(unreachableEndpoint: true)
        await game.waitForRow(Self.opening)
        XCTAssertEqual(game.presenter.header.location, "Kilteevan Village")

        await game.submit("/look")
        await game.waitForRow("A muddy road runs between low stone walls")
        await game.submit("/exits")
        await game.waitForRow("Connolly Cottage (4 min on foot)")
        await game.submit("go to Connolly Cottage")
        await waitUntil { game.presenter.header.location == "Connolly Cottage" }
        await game.submit("/people")
        await game.waitForRow("Smallholder and cattle drover")
        XCTAssertFalse(game.presenter.showsRetry, "No local command needed an Endpoint")

        await game.submit("ask Mícheál about the cattle")
        await game.waitForRow("The road out of the parish is washed away", timeout: 60)
        await waitUntil { game.presenter.showsRetry }

        await game.submit("go to Kilteevan Village")
        await waitUntil { game.presenter.header.location == "Kilteevan Village" }
        await game.submit("/exits")
        await game.waitForRow("Letter Office")
    }

    /// Was `RundalePhase3UITests.testReplyNamingAnUnknownPersonIsCommittedAsWritten`.
    func testReplyNamingAnUnknownPersonIsCommittedAsWritten() async throws {
        try await game.launch()
        await game.goToTheCottage()
        await game.submit("Mícheál, who is minding the cattle?")
        let reply = "Tadhg Mulvihill is minding them for me, and he has the grazing by the river."
        await waitUntil(timeout: 60) { game.committedReplies(containing: reply) == 1 }
        try await game.relaunch()
        await waitUntil(timeout: 30) { game.committedReplies(containing: reply) == 1 }
    }

    /// Was `RundalePhase3ClarificationUITests.testAmbiguousConnollyRequiresSelectionBeforeEndpointWork`.
    func testAmbiguousConnollyRequiresSelectionBeforeEndpointWork() async throws {
        try await game.launch()
        await game.goToTheCottage()
        await game.submit("ask Connolly about the household")
        await waitUntil { game.presenter.clarification != nil }
        let options = try XCTUnwrap(game.presenter.clarification).options.map(\.id)
        XCTAssertTrue(options.contains("choose-npc-2"))
        XCTAssertTrue(options.contains("choose-npc-3"))
        XCTAssertTrue(game.hasRow("Which Connolly do you mean?"))
        XCTAssertFalse(game.presenter.showsStop, "nothing is sent until the player answers")
        game.presenter.selectClarification(try XCTUnwrap(game.presenter.clarification?.options.first { $0.id == "choose-npc-3" }))

        await waitUntil { !game.hasRow("Which Connolly do you mean?") && game.presenter.clarification == nil }
        await game.waitForRow("household work allows", timeout: 60)
    }

    /// Was `RundalePhase3ClarificationUITests.testUnaddressedSpeechAsksWhoAndTheChoiceBecomesThePartner`.
    func testUnaddressedSpeechAsksWhoAndTheChoiceBecomesThePartner() async throws {
        try await game.launch()
        await game.goToTheCottage()
        await game.submit("Hello")
        await waitUntil { game.presenter.clarification != nil }
        let clarification = try XCTUnwrap(game.presenter.clarification)
        XCTAssertTrue(game.hasRow("Who are you speaking to?"))
        XCTAssertTrue(clarification.options.contains { $0.id == "choose-npc-2" })
        let everyone = clarification.options.first { $0.id == "choose-all" }
        XCTAssertEqual(everyone?.accessibilityLabel ?? everyone?.label, "Everyone here")
        XCTAssertFalse(game.presenter.showsStop, "nothing is sent until the player answers")
        game.presenter.selectClarification(try XCTUnwrap(clarification.options.first { $0.id == "choose-npc-3" }))

        await waitUntil { game.presenter.clarification == nil }
        await game.waitForRow("household work allows", timeout: 60)
        await waitUntil { !game.presenter.isStreaming }
        await game.submit("And how's the harvest?")
        await waitUntil(timeout: 60) { game.committedReplies(containing: "household work allows") == 2 }
        XCTAssertNil(game.presenter.clarification, "the chosen partner answers without asking again")
    }

    // MARK: Phase 4

    /// Was `RundalePhase4NetworkUITests.testConnectionLossBeforeResponseCanRetryWithoutRestart`.
    func testConnectionLossBeforeTheResponseCanRetryWithoutRestart() async throws {
        try await assertNetworkRecovery(command: "ask Mícheál offline once", partialExpected: false)
    }

    /// Was `RundalePhase4NetworkUITests.testConnectionLossDuringStreamCanRetryWithoutDuplicatingDialogue`.
    func testConnectionLossDuringTheStreamCanRetryWithoutDuplicatingDialogue() async throws {
        try await assertNetworkRecovery(command: "ask Mícheál disconnect once", partialExpected: true)
    }

    private func assertNetworkRecovery(command: String, partialExpected: Bool) async throws {
        try await game.launch()
        await game.goToTheCottage()
        await game.submit(command)
        await waitUntil(timeout: 60) { game.presenter.showsRetry }
        XCTAssertEqual(game.completedDialogue.count, 0)
        if partialExpected {
            let partial = try XCTUnwrap(game.dialogue(containing: "The wet ground"))
            XCTAssertTrue(partial.accessibilityLabel.contains("not applied"), partial.accessibilityLabel)
        }
        XCTAssertTrue(game.hasRow("The road out of the parish is washed away"))
        XCTAssertFalse(game.presenter.showsStop)

        game.presenter.retryLastFailed()
        await waitUntil(timeout: 60) { game.completedDialogue.count == 1 }
        await waitUntil { !game.presenter.isStreaming }
        XCTAssertEqual(game.playerCommands(containing: command), 1)
        XCTAssertFalse(game.presenter.showsRetry)

        try await game.relaunch()
        await waitUntil { game.playerCommands(containing: command) == 1 }
        XCTAssertEqual(game.completedDialogue.count, 1)
    }
}

/// A fail-always Endpoint transport: every request fails as it would with the
/// network down. The old UI test used the real client against an unresolvable
/// host; the client's own failure mapping is tested in `RundaleSessionTests`
/// (`EndpointFailureClassificationTests`).
private struct UnreachableTransport: EndpointTransport {
    func open(_ request: URLRequest) -> EndpointByteStream {
        EndpointByteStream(events: AsyncThrowingStream<EndpointTransportEvent, Error> {
            $0.finish(throwing: URLError(.cannotFindHost))
        })
    }
}

private struct StaticTestCredentials: LimerickEndpointKit.EndpointCredentialProvider {
    func credentials() async throws -> LimerickEndpointKit.EndpointCredentials {
        LimerickEndpointKit.EndpointCredentials(authorizationToken: "scenario", appCheckToken: "scenario")
    }
}

/// One game on disk and the presenter playing it. `relaunch` drops the
/// presenter and opens a new one over the same save.
@MainActor
private final class Game {
    let directory: URL
    let openingText = "A muddy road runs between low stone walls"
    private(set) var presenter: RundaleSessionPresenter!
    private var controller: RundaleEngineController?
    private var arguments: [String] = []
    private var bundle: [String: Any] = [:]
    private var screenshot: Data?
    private var unreachable = false

    init(directory: URL) {
        self.directory = directory
    }

    func launch(
        bundle: [String: Any] = [:],
        screenshot: Data? = nil,
        unreachableEndpoint: Bool = false
    ) async throws {
        arguments = ["--ui-tests", "--phase3", "--no-auto-focus",
                     "--draft-file=\(directory.appendingPathComponent("projection.json").path)"]
        if !unreachableEndpoint { arguments.append("--phase3-mock") }
        self.bundle = bundle
        self.screenshot = screenshot
        unreachable = unreachableEndpoint
        try await open()
    }

    func relaunch() async throws {
        // The app takes this snapshot when it moves to the background.
        await presenter.persistLifecycleSnapshot()
        weak let previous = presenter
        weak let previousSession = controller
        presenter = nil
        controller = nil
        await waitUntil { previous == nil && previousSession == nil }
        try await open()
    }

    private func open() async throws {
        let launch = LaunchConfiguration(
            arguments: arguments,
            environment: unreachable ? ["RUNDALE_ENDPOINT_BASE_URL": "https://endpoints.invalid"] : [:],
            bundle: bundle
        )
        let mod = try XCTUnwrap(Bundle.main.url(forResource: "rundale", withExtension: nil, subdirectory: "Mods"))
        let session = RundaleEngineController(
            configuration: launch,
            credentials: { StaticTestCredentials() },
            modDirectory: mod,
            log: { _ in },
            transport: unreachable ? UnreachableTransport() : nil
        )
        let screenshot = screenshot
        controller = session
        presenter = RundaleSessionPresenter(
            launch: launch,
            session: session,
            bugReports: RundaleSessionPresenter.defaultBugReports(launch) { StaticTestCredentials() },
            captureScreenshot: { screenshot },
            deviceDescription: "test"
        )
        presenter.start()
        await waitUntil { !self.presenter.transcript.isEmpty || session.persistenceError != nil }
        XCTAssertNil(session.persistenceError, session.persistenceDiagnostic ?? "")
    }

    func type(_ text: String) {
        presenter.draft = text
        presenter.noteDraftMutation()
        presenter.refreshCompletions()
    }

    /// Sends `command` and waits for the engine to accept it. A turn is
    /// accepted when its command row appears (the composer clears, but a
    /// failed turn can return the text to it at once); a host command such as
    /// `/new` makes no row, so it is accepted when the composer clears.
    func submit(_ command: String, makesARow: Bool = true) async {
        let rows = playerCommands(containing: command)
        type(command)
        presenter.submitDraft()
        if makesARow {
            await waitUntil { self.playerCommands(containing: command) > rows }
            if playerCommands(containing: command) <= rows {
                XCTFail("not accepted: message=\(presenter.submissionMessage ?? "nil") streaming=\(presenter.isStreaming) draft=\(presenter.draft) tail=\(labels.suffix(3))")
            }
        } else {
            await waitUntil { self.presenter.draft.isEmpty }
        }
    }

    /// Picks a completion the way the strip does, then lets the view's draft
    /// change handler refresh what is offered next.
    func select(_ id: String, file: StaticString = #filePath, line: UInt = #line) {
        guard let completion = presenter.completions.first(where: { $0.id == id }) else {
            return XCTFail("no completion \(id) in \(completionIDs)", file: file, line: line)
        }
        presenter.selectCompletion(completion)
        presenter.refreshCompletions()
    }

    func goToTheCottage() async {
        await waitForRow(openingText)
        await submit("go to Connolly Cottage")
        await waitForRow("A peat fire warms the single room")
    }

    var labels: [String] { presenter.transcript.map(\.accessibilityLabel) }
    var completionIDs: [String] { presenter.completions.map(\.id) }

    func hasRow(_ text: String) -> Bool {
        presenter.transcript.contains { $0.accessibilityLabel.contains(text) }
    }

    func waitForRow(_ text: String, timeout: TimeInterval = 30) async {
        await waitUntil(timeout: timeout) { self.hasRow(text) }
    }

    func sceneLabel(_ name: String, _ text: String) -> String? {
        labels.first { $0.hasPrefix("Scene. \(name). \(text)") }
    }

    func assertNoRestatedHeader(_ label: String, file: StaticString = #filePath, line: UInt = #line) {
        for restated in ["It is morning", "weather is", "sky hangs", "You can go to"] {
            XCTAssertFalse(label.contains(restated), "\(restated) in \(label)", file: file, line: line)
        }
    }

    func dialogue(containing text: String) -> PresentedTranscriptItem? {
        presenter.transcript.first { $0.kind == .npcDialogue && $0.text.contains(text) }
    }

    func committedReplies(containing text: String) -> Int {
        presenter.transcript.filter { $0.kind == .npcDialogue && $0.state == .committed && $0.text.contains(text) }.count
    }

    func playerCommands(containing text: String) -> Int {
        presenter.transcript.filter { $0.kind == .playerCommand && $0.text.contains(text) }.count
    }

    var completedDialogue: [PresentedTranscriptItem] {
        presenter.transcript.filter {
            $0.kind == .npcDialogue && $0.state == .committed
                && ($0.text.contains("difficult this week") || $0.text.contains("stands beyond the alder trees"))
        }
    }
}

@MainActor
private func waitUntil(
    timeout: TimeInterval = 30,
    _ condition: @MainActor () -> Bool,
    file: StaticString = #filePath,
    line: UInt = #line
) async {
    let deadline = Date().addingTimeInterval(timeout)
    while Date() < deadline {
        if condition() { return }
        try? await Task.sleep(nanoseconds: 20_000_000)
    }
    XCTFail("condition did not become true within \(timeout) s", file: file, line: line)
}
