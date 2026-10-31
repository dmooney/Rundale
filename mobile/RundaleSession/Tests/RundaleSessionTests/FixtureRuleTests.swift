import Foundation
import RundaleKit
import XCTest
@testable import RundaleSession

/// The composer and transcript rules the iPhone UI suites used to check by
/// driving the app through the accessibility tree (#2319). Each test runs the
/// real presenter over the deterministic fixture controller, the same pair the
/// app launches for `--fixture=<name>`, so a rule is checked in milliseconds
/// and on Linux. What these tests cannot see (layout, the keyboard, gestures,
/// the process lifecycle) stays in `RundaleUITests`.
@MainActor
final class FixtureRuleTests: XCTestCase {
    private var directory: URL!

    override func setUp() async throws {
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("FixtureRuleTests-\(UUID().uuidString)")
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: directory)
    }

    /// A presenter started the way the UI tests launch the fixture: manual
    /// stepping with `--ui-tests`, or the automatic scheduler without it.
    private func presenter(
        fixture: String = "standard",
        manual: Bool = true,
        reset: Bool = true
    ) async -> RundaleSessionPresenter {
        var arguments = ["--fixture=\(fixture)",
                         "--draft-file=\(directory.appendingPathComponent("phase1-draft.json").path)"]
        if manual { arguments.append("--ui-tests") }
        if reset { arguments.append("--reset-fixture") }
        let launch = LaunchConfiguration(arguments: arguments, environment: [:], bundle: [:])
        let presenter = RundaleSessionPresenter(
            launch: launch, session: RundaleFixtureController(configuration: launch), bugReports: nil,
            captureScreenshot: { nil }, deviceDescription: "test"
        )
        presenter.start()
        await waitUntil { !presenter.transcript.isEmpty }
        return presenter
    }

    /// Types `text` the way the composer does: set the draft, then the view's
    /// change handlers note the mutation and refresh the completions.
    private func type(_ text: String, into presenter: RundaleSessionPresenter) {
        presenter.draft = text
        presenter.noteDraftMutation()
        presenter.refreshCompletions()
    }

    private func send(_ text: String, on presenter: RundaleSessionPresenter) async {
        type(text, into: presenter)
        presenter.submitDraft()
        await waitUntil { presenter.transcript.contains { $0.kind == .playerCommand && $0.text == text } }
    }

    /// Taps Next until the fixture reaches a terminal state, as the UI tests'
    /// `finishManualStream` does.
    private func finish(_ presenter: RundaleSessionPresenter) async {
        for _ in 0..<12 {
            await waitUntil { presenter.isStreaming || presenter.draft.isEmpty }
            guard presenter.showsFixtureStep else { break }
            presenter.advanceFixture()
            try? await Task.sleep(nanoseconds: 20_000_000)
        }
        await waitUntil { !presenter.isStreaming }
    }

    private func label(_ presenter: RundaleSessionPresenter, containing text: String) -> String? {
        presenter.transcript.map(\.accessibilityLabel).first { $0.contains(text) }
    }

    private func completionIDs(_ presenter: RundaleSessionPresenter) -> [String] {
        presenter.completions.map(\.id)
    }

    // MARK: Composer

    /// Was `testMultilineSubmissionClearsOnlyAcceptedDraft` and
    /// `testAcceptedCommandRowIsExactAndImmediatelyVisible`.
    func testAcceptedCommandRowIsExactAndTheDraftClears() async {
        let presenter = await presenter()
        let command = "look around\ncheck the road\nwait by the gate"
        await send(command, on: presenter)
        XCTAssertEqual(presenter.draft, "")
        XCTAssertNotNil(label(presenter, containing: "Player command. \(command)"))
        XCTAssertTrue(presenter.isFollowingNewest, "an accepted row is shown at the newest end")

        await finish(presenter)
        let single = "look around"
        await send(single, on: presenter)
        XCTAssertTrue(presenter.transcript.map(\.accessibilityLabel).contains("Player command. \(single)"))
    }

    /// Was `testSendButtonExposesDistinctDisabledAndEnabledStates`.
    func testSendStatesAreDistinctForAnEmptyAndAFilledDraft() async {
        let presenter = await presenter()
        XCTAssertFalse(presenter.canSubmitDraft)
        XCTAssertEqual(presenter.sendAccessibilityValue, "Enter a command to enable")
        type("Look", into: presenter)
        XCTAssertTrue(presenter.canSubmitDraft)
        XCTAssertEqual(presenter.sendAccessibilityValue, "Ready to send")
        type("  \n ", into: presenter)
        XCTAssertFalse(presenter.canSubmitDraft)
        XCTAssertEqual(presenter.sendAccessibilityValue, "Enter a command to enable")
    }

    /// Was `testDraftTypedDuringAcceptedStreamSurvivesStop`.
    func testDraftTypedWhileAReplyStreamsSurvivesStop() async {
        let presenter = await presenter(fixture: "manual-stream")
        type("long stream", into: presenter)
        presenter.submitDraft()
        await waitUntil { presenter.showsFixtureStep }

        type("draft typed while the response runs", into: presenter)
        presenter.stop()
        await waitUntil { !presenter.isStreaming }
        XCTAssertEqual(presenter.draft, "draft typed while the response runs")
    }

    /// Was `testStopRetainsExactPartialRowAndRemovesManualAdvance`.
    func testStopRetainsTheExactPartialRowAndRemovesTheManualAdvance() async {
        let presenter = await presenter()
        await send("long stream", on: presenter)
        await waitUntil { presenter.showsFixtureStep }
        presenter.advanceFixture()
        await waitUntil { presenter.transcript.contains { $0.kind == .commandInterpreted } }
        presenter.advanceFixture()
        let partial = "Dialogue. Peig. The first part arrives.. In progress"
        await waitUntil { presenter.transcript.map(\.accessibilityLabel).contains(partial) }
        let partialID = presenter.transcript.first { $0.accessibilityLabel == partial }?.id

        presenter.stop()
        await waitUntil { !presenter.isStreaming }
        let retained = presenter.transcript.first { $0.id == partialID }
        XCTAssertEqual(retained?.accessibilityLabel, "Dialogue. Peig. The first part arrives.. Interrupted; not applied")
        XCTAssertFalse(presenter.showsFixtureStep, "Stop removes the only manual stream advance")
        XCTAssertNil(label(presenter, containing: "Then the road opens into rain and light."))
        XCTAssertNil(label(presenter, containing: "At last, the whole thought is clear."))
        XCTAssertTrue(presenter.canSubmitDraft == false)
        type("/look", into: presenter)
        XCTAssertTrue(presenter.canSubmitDraft, "the composer is usable again after Stop")
    }

    /// Was `testWaitingKnotAppearsDuringRequestAndDisappearsOnStopAndCompletion`.
    func testWaitingKnotShowsOnlyWhileARequestRunsAndNotInACompactLayout() async {
        let presenter = await presenter()
        XCTAssertFalse(presenter.showsWaitingKnot(compactLayout: false))
        await send("Hello", on: presenter)
        await waitUntil { presenter.isStreaming }
        XCTAssertTrue(presenter.showsWaitingKnot(compactLayout: false))
        XCTAssertFalse(presenter.showsWaitingKnot(compactLayout: true), "a compact layout keeps the room")
        XCTAssertTrue(presenter.showsStop)

        presenter.stop()
        await waitUntil { !presenter.isStreaming }
        XCTAssertFalse(presenter.showsWaitingKnot(compactLayout: false))

        await send("Hello again", on: presenter)
        await finish(presenter)
        XCTAssertFalse(presenter.showsWaitingKnot(compactLayout: false))
    }

    // MARK: Completions

    /// Was `testCompletionInsertionAndSlashCommandSubmission`.
    func testCompletionInsertsTheNameAndASlashCommandSubmits() async {
        let presenter = await presenter()
        type("@", into: presenter)
        XCTAssertTrue(completionIDs(presenter).contains("npc-micheal"))
        presenter.selectCompletion(presenter.completions.first { $0.id == "npc-micheal" }!)
        XCTAssertTrue(presenter.draft.contains("Mícheál"), presenter.draft)
        XCTAssertTrue(presenter.completions.isEmpty, "a chosen completion closes the strip")

        type("/", into: presenter)
        let first = try? XCTUnwrap(presenter.completions.first)
        XCTAssertNotNil(first)
        presenter.selectCompletion(first!)
        XCTAssertTrue(presenter.draft.hasPrefix("/"))
        presenter.submitDraft()
        await waitUntil { presenter.transcript.contains { $0.kind == .playerCommand && $0.text.hasPrefix("/") } }
    }

    /// Was `testPeopleAndCommandsButtonsAvoidSymbolKeyboardAndPreserveDraftUntilSelection`.
    func testPeopleAndCommandsShortcutsKeepTheDraftUntilASelection() async {
        let presenter = await presenter()
        type("Could you help me?", into: presenter)
        presenter.browseCompletions("@")
        XCTAssertEqual(presenter.draft, "Could you help me?", "browsing does not type an at sign")
        XCTAssertTrue(completionIDs(presenter).contains("npc-micheal"))
        presenter.selectCompletion(presenter.completions.first { $0.id == "npc-micheal" }!)
        XCTAssertEqual(presenter.draft, "@Mícheál Connolly Could you help me?")

        presenter.browseCompletions("/")
        XCTAssertEqual(presenter.draft, "@Mícheál Connolly Could you help me?")
        let look = presenter.completions.first { $0.id == "look" }
        XCTAssertNotNil(look)
        XCTAssertEqual(look?.detail, "Replace draft")
        presenter.selectCompletion(look!)
        XCTAssertEqual(presenter.draft, "/look")
        presenter.submitDraft()
        await waitUntil { presenter.transcript.contains { $0.kind == .playerCommand && $0.text == "/look" } }
    }

    // MARK: Recall, clarification, history

    /// Was `testTranscriptCommandRecallWorksWithoutAHistoryButton` and
    /// `testRecallPreservesOriginalExactRowAndCompletionRendersExactResult`.
    func testRecalledCommandKeepsItsRowAndCompletionRendersTheExactResult() async {
        let presenter = await presenter()
        await send("look around", on: presenter)
        await finish(presenter)
        await send("walk to the bridge", on: presenter)
        await finish(presenter)

        let original = presenter.transcript.first { $0.kind == .playerCommand && $0.text == "walk to the bridge" }
        let originalID = original?.id
        XCTAssertNotNil(originalID)
        presenter.recallCommand(original!.text)
        XCTAssertEqual(presenter.draft, "walk to the bridge")
        type(presenter.draft + " slowly", into: presenter)
        presenter.submitDraft()
        await waitUntil { presenter.transcript.contains { $0.text == "walk to the bridge slowly" } }
        await finish(presenter)

        let revised = presenter.transcript.first { $0.text == "walk to the bridge slowly" }
        XCTAssertNotEqual(revised?.id, originalID, "the revised command is a new row")
        XCTAssertEqual(presenter.transcript.first { $0.id == originalID }?.accessibilityLabel,
                       "Player command. walk to the bridge")
        XCTAssertEqual(revised?.accessibilityLabel, "Player command. walk to the bridge slowly")

        presenter.browseCompletions("/")
        presenter.selectCompletion(presenter.completions.first { $0.id == "look" }!)
        XCTAssertEqual(presenter.draft, "/look")
        presenter.submitDraft()
        await waitUntil { presenter.transcript.contains { $0.text == "/look" } }
        await finish(presenter)
        XCTAssertTrue(presenter.transcript.map(\.accessibilityLabel)
            .contains("Result. Crossroads · stone wall · lane to the east"))
    }

    /// Was `testClarificationOptionIsTemporaryAndActionable`.
    func testClarificationIsActionableThenGoneAndThePromptRowCollapses() async {
        let presenter = await presenter(fixture: "clarification")
        await send("ambiguous", on: presenter)
        await waitUntil { presenter.showsFixtureStep }
        presenter.advanceFixture()
        presenter.advanceFixture()
        await waitUntil { presenter.clarification != nil }
        let clarification = presenter.clarification!
        XCTAssertFalse(presenter.showsStop, "a question waiting on the player is not work")
        XCTAssertNotNil(label(presenter, containing: "I’m not sure which Connolly you mean."))
        let option = clarification.options.first!
        presenter.selectClarification(option)
        await waitUntil { presenter.clarification == nil }
        await waitUntil { self.label(presenter, containing: "Directed to \(option.label).") != nil }
        XCTAssertNil(label(presenter, containing: "I’m not sure which Connolly you mean."),
                     "the answered prompt leaves the transcript")
    }

    /// Was `testAcceptedMessagesFollowLatestAcrossRepeatedTurnsAndHistoryReading`.
    func testAcceptedMessagesRejoinTheLatestExchangeAcrossRepeatedTurns() async {
        let presenter = await presenter(fixture: "long-history")
        XCTAssertTrue(presenter.isFollowingNewest)
        for (index, command) in ["first new exchange", "second new exchange", "third new exchange"].enumerated() {
            if index == 1 {
                presenter.readHistory(anchor: TranscriptAnchor(
                    itemID: TranscriptItemID(presenter.transcript.first!.id), offset: 10
                ))
                await waitUntil { !presenter.isFollowingNewest }
            }
            await send(command, on: presenter)
            await waitUntil { presenter.isFollowingNewest }
            let before = presenter.streamRevision
            await waitUntil { presenter.showsFixtureStep }
            presenter.advanceFixture()
            await waitUntil { presenter.streamRevision > before }
            XCTAssertTrue(presenter.isFollowingNewest, "a growing reply must not enable history mode")
            await finish(presenter)
            XCTAssertTrue(presenter.isFollowingNewest)
        }
    }

    /// Was `testFollowingNewestStartsAtLatestAndTracksComposerResize`: a
    /// launch follows the newest entry, and a longer draft does not move it.
    func testALaunchFollowsTheNewestEntryAndAGrowingDraftDoesNotChangeIt() async {
        let presenter = await presenter(fixture: "long-history")
        await waitUntil { presenter.transcript.last?.text == "Historical fixture entry 180." }
        XCTAssertTrue(presenter.isFollowingNewest)
        type("first line\nsecond line\nthird line\nfourth line", into: presenter)
        XCTAssertTrue(presenter.isFollowingNewest)
        XCTAssertEqual(presenter.transcript.last?.text, "Historical fixture entry 180.")
    }

    /// Was `testOrdinaryLaunchRestoresHistoryAndDraftAcrossRelaunch` at the
    /// rule level (the UI journey keeps the product launch path).
    func testRelaunchRestoresHistoryAndDraft() async {
        let first = await presenter()
        await send("look around", on: first)
        await finish(first)
        type("ordinary launch keeps this note", into: first)
        await first.persistLifecycleSnapshot()

        let second = await presenter(reset: false)
        XCTAssertTrue(second.transcript.contains { $0.kind == .playerCommand && $0.text == "look around" })
        XCTAssertEqual(second.draft, "ordinary launch keeps this note")
    }

    /// Was `testTranscriptKindsAndFocusLoopUseProductionControls`: each kind
    /// reads with its own spoken label.
    func testTranscriptKindsHaveTheirSpokenLabels() async {
        let presenter = await presenter()
        let labels = presenter.transcript.map(\.accessibilityLabel)
        XCTAssertTrue(labels.contains { $0.hasPrefix("Scene. ") })
        XCTAssertTrue(labels.contains { $0.hasPrefix("Narration. ") })
        await send("long stream", on: presenter)
        XCTAssertTrue(presenter.transcript.map(\.accessibilityLabel).contains("Player command. long stream"))
        await finish(presenter)
        XCTAssertTrue(presenter.transcript.map(\.accessibilityLabel).contains { $0.hasPrefix("Interpretation. ") })
        XCTAssertTrue(presenter.transcript.map(\.accessibilityLabel).contains(
            "Dialogue. Peig. The first part arrives. Then the road opens into rain and light. At last, the whole thought is clear."
        ))
        XCTAssertEqual(
            PresentedHeader(location: "Kilteevan Village", timeOfDay: "Morning", weather: "Clear").accessibilityLabel,
            "Current place, Kilteevan Village. Morning. Clear."
        )
        XCTAssertEqual(presenter.header.accessibilityLabel.hasPrefix("Current place, "), true)
    }

    // MARK: Automatic delivery (the real scheduler)

    /// Was `testAutomaticChunksStayVisibleAndCompleteWithoutNext`,
    /// `testAutomaticChunksWorkWithNativeMultilineComposer`,
    /// `testAutomaticStopRetainsExactPartialBeyondNextTimerTicks` and
    /// `testAutomaticOutputPreservesHistoryAndNewTextReturnsToFinalReply`.
    /// The scheduler steps every 2.75 s, so successive chunks arrive on one row
    /// and the reply completes with no Next control.
    func testAutomaticChunksShareOneRowCompleteWithoutNextAndStopRetainsThePartial() async {
        let first = "The first part arrives."
        let second = "The first part arrives. Then the road opens into rain and light."
        let final = "The first part arrives. Then the road opens into rain and light. At last, the whole thought is clear."
        let presenter = await presenter(fixture: "long-history", manual: false)
        XCTAssertFalse(presenter.showsFixtureStep, "the scheduler, not a Next control, advances the reply")
        let readerRow = presenter.transcript[10].id
        type("long stream", into: presenter)
        presenter.submitDraft()
        await waitUntil(timeout: 20) { presenter.transcript.contains { $0.accessibilityLabel == "Dialogue. Peig. \(first). In progress" } }
        let identity = presenter.transcript.first { $0.text == first }?.id
        XCTAssertNotNil(identity)
        XCTAssertTrue(presenter.showsStop)
        XCTAssertFalse(presenter.showsFixtureStep)

        // A reader who leaves the newest end keeps their place while chunks arrive.
        presenter.readHistory(anchor: TranscriptAnchor(itemID: TranscriptItemID(readerRow), offset: 12))
        await waitUntil { !presenter.isFollowingNewest }
        let readingAnchor = presenter.initialTranscriptAnchor
        let before = Date()
        await waitUntil(timeout: 20) { presenter.transcript.contains { $0.text == second } }
        XCTAssertEqual(presenter.transcript.first { $0.text == second }?.id, identity, "chunks update one row")
        XCTAssertGreaterThan(Date().timeIntervalSince(before), 1, "the chunks are paced, not delivered at once")
        XCTAssertFalse(presenter.isFollowingNewest)
        XCTAssertEqual(presenter.initialTranscriptAnchor, readingAnchor, "streaming keeps the reading anchor")
        await waitUntil(timeout: 20) { presenter.transcript.contains { $0.text == final } }
        XCTAssertEqual(presenter.transcript.first { $0.text == final }?.id, identity)
        await waitUntil(timeout: 20) { !presenter.isStreaming }
        XCTAssertEqual(presenter.transcript.first { $0.id == identity }?.accessibilityLabel, "Dialogue. Peig. \(final)")
        XCTAssertFalse(presenter.showsStop)
        presenter.followNewest()
        await waitUntil { presenter.isFollowingNewest }

        // A second reply stopped after its first chunk keeps exactly that chunk
        // beyond the scheduler's next ticks.
        type("long stream", into: presenter)
        presenter.submitDraft()
        await waitUntil(timeout: 20) {
            presenter.transcript.filter { $0.text == first && $0.isProvisional }.count == 1
        }
        let partialID = presenter.transcript.last { $0.text == first }?.id
        presenter.stop()
        await waitUntil { !presenter.isStreaming }
        let stopped = "Dialogue. Peig. \(first). Interrupted; not applied"
        XCTAssertEqual(presenter.transcript.first { $0.id == partialID }?.accessibilityLabel, stopped)
        try? await Task.sleep(nanoseconds: 3_500_000_000)
        XCTAssertEqual(presenter.transcript.first { $0.id == partialID }?.accessibilityLabel, stopped,
                       "no later timer tick changes a stopped reply")
        XCTAssertFalse(presenter.isStreaming)
        type("/look", into: presenter)
        XCTAssertTrue(presenter.canSubmitDraft)
    }
}
