import Combine
import XCTest
import RundaleKit
@testable import Rundale

@MainActor
final class RundalePhase4Tests: XCTestCase {
    func testBackgroundingStopsActiveRequestBeforeLifecyclePersistence() async {
        let session = Phase4TestSession(active: true)
        let model = RundalePresentationModel(
            launch: LaunchConfiguration(arguments: ["--fixture=standard"], environment: [:], bundle: [:]),
            session: session
        )

        model.handleBackgrounding()
        await waitUntil { session.lifecycleEvents.count == 2 }

        XCTAssertEqual(session.lifecycleEvents, ["stop", "persist"])
        XCTAssertEqual(session.stopCalls, 1)
    }

    func testBackgroundingBlocksAQueuedSubmitUntilForeground() async {
        let session = Phase4TestSession(active: false)
        let model = RundalePresentationModel(
            launch: LaunchConfiguration(arguments: ["--fixture=standard"], environment: [:], bundle: [:]),
            session: session
        )

        model.draft = "look around"
        model.noteDraftMutation()
        model.handleBackgrounding()
        model.submitDraft()
        await Task.yield()
        XCTAssertEqual(session.submitCalls, 0)

        model.handleForegrounding()
        await waitUntil { session.inferenceAllowed }
        model.submitDraft()
        await waitUntil { session.submitCalls == 1 }
        XCTAssertEqual(session.submitCalls, 1)
    }

    func testRapidBackgroundForegroundStopsOldRequestBeforeAllowingNewSubmission() async {
        let session = Phase4TestSession(active: true)
        let model = RundalePresentationModel(
            launch: LaunchConfiguration(arguments: ["--fixture=standard"], environment: [:], bundle: [:]),
            session: session
        )
        model.handleBackgrounding()
        model.handleForegrounding()
        await waitUntil { session.inferenceAllowed }
        XCTAssertEqual(session.stopCalls, 1)
        XCTAssertEqual(session.lifecycleEvents, ["stop", "persist"])
        model.draft = "next question"
        model.noteDraftMutation()
        model.submitDraft()
        await waitUntil { session.submitCalls == 1 }
        XCTAssertEqual(session.stopCalls, 1, "Old background work must not cancel the new request")
    }

    func testAcceptedReceiptRacingBackgroundStillClearsTheOriginalDraft() async {
        let session = Phase4TestSession(active: false)
        session.acceptedRequestRemainsActive = true
        session.gateSubmit = true
        let model = RundalePresentationModel(
            launch: LaunchConfiguration(arguments: ["--fixture=standard"], environment: [:], bundle: [:]),
            session: session
        )

        model.draft = "look around"
        model.noteDraftMutation()
        model.submitDraft()
        await waitUntil { session.submitStarted }
        model.handleBackgrounding()
        session.releaseSubmit()
        await waitUntil { session.stopCalls == 1 && session.lifecycleEvents.contains("persist") }

        XCTAssertEqual(model.draft, "")
        XCTAssertEqual(session.submitCalls, 1)
    }

    func testRetypingTheSameTextDuringAcceptanceKeepsTheNewDraft() async {
        let session = Phase4TestSession(active: false)
        session.gateSubmit = true
        let model = RundalePresentationModel(
            launch: LaunchConfiguration(arguments: ["--fixture=standard"], environment: [:], bundle: [:]),
            session: session
        )

        model.draft = "look around"
        model.noteDraftMutation()
        model.noteDraftMutation()
        model.submitDraft()
        await waitUntil { session.submitStarted }
        model.draft = "look around"
        model.noteDraftMutation()
        session.releaseSubmit()
        await waitUntil { !model.isStreaming }

        XCTAssertEqual(model.draft, "look around")
    }

    func testAcceptedSubmissionFromHistoryReturnsToNewest() async {
        let session = Phase4TestSession(active: false)
        session.setReadingHistory()
        let model = RundalePresentationModel(
            launch: LaunchConfiguration(arguments: ["--fixture=standard"], environment: [:], bundle: [:]),
            session: session
        )

        model.draft = "look around"
        model.noteDraftMutation()
        model.submitDraft()
        await waitUntil { session.followNewestCalls == 1 }

        XCTAssertEqual(session.followNewestCalls, 1)
        XCTAssertTrue(model.isFollowingNewest)
    }

    func testRejectedSubmissionFromHistoryKeepsReadingHistory() async {
        let session = Phase4TestSession(active: false)
        session.setReadingHistory()
        session.acceptedReceipt = false
        let model = RundalePresentationModel(
            launch: LaunchConfiguration(arguments: ["--fixture=standard"], environment: [:], bundle: [:]),
            session: session
        )

        model.draft = "look around"
        model.noteDraftMutation()
        model.submitDraft()
        await waitUntil { session.submitCalls == 1 }

        XCTAssertEqual(session.followNewestCalls, 0)
        XCTAssertFalse(session.state.viewport.isFollowingNewest)
        XCTAssertFalse(model.isFollowingNewest)
    }

    func testBackgroundDuringRetryStopsTheLatestFailedRequest() async {
        let session = Phase4TestSession(active: false, failedRequests: 2)
        session.gateSubmit = true
        let model = RundalePresentationModel(
            launch: LaunchConfiguration(arguments: ["--fixture=standard"], environment: [:], bundle: [:]),
            session: session
        )
        model.retryLastFailed()
        await waitUntil { session.submitStarted }
        model.handleBackgrounding()
        session.releaseSubmit()
        await waitUntil { session.stopCalls == 1 }
        XCTAssertNil(session.state.activeRequestID)
    }

    func testMalformedProjectionIsReportedWithoutStartingFromSilentEmptyState() throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("rundale-phase4-\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: url) }
        let original = Data("not-json".utf8)
        try original.write(to: url)

        let configuration = LaunchConfiguration(
            arguments: ["--phase3", "--draft-file=\(url.path)"],
            environment: [:],
            bundle: [:]
        )
        let controller = RundaleEngineController(configuration: configuration)

        XCTAssertTrue(controller.persistenceError?.contains("could not be restored") == true)
        XCTAssertNotNil(controller.persistDraft("must not overwrite"))
        let after = try Data(contentsOf: url)
        XCTAssertEqual(after, original)
    }

    private func waitUntil(
        _ condition: @escaping @MainActor () -> Bool,
        file: StaticString = #filePath,
        line: UInt = #line
    ) async {
        for _ in 0..<100 {
            if condition() { return }
            await Task.yield()
        }
        XCTFail("condition did not become true", file: file, line: line)
    }
}

@MainActor
private final class Phase4TestSession: RundaleSessionControlling {
    private var presentation: PresentationSession
    private let subject = CurrentValueSubject<SessionState, Never>(SessionState())

    private(set) var state: SessionState
    private(set) var stopCalls = 0
    private(set) var submitCalls = 0
    private(set) var followNewestCalls = 0
    private(set) var submitStarted = false
    var gateSubmit = false
    var acceptedRequestRemainsActive = false
    var acceptedReceipt = true
    private(set) var inferenceAllowed = true
    private var submitContinuation: CheckedContinuation<Void, Never>?
    private(set) var lifecycleEvents: [String] = []

    init(active: Bool, failedRequests: Int = 0) {
        let request = RequestRecord(
            id: LogicalRequestID("request"),
            originalText: "look around",
            attempts: [RequestAttempt(
                id: ExecutionAttemptID("attempt"),
                originalText: "look around",
                phase: .executing,
                startedAt: EventSequence(1)
            )],
            currentAttemptID: ExecutionAttemptID("attempt"),
            phase: .executing
        )
        state = active
            ? SessionState(requests: [request], activeRequestID: request.id)
            : SessionState(requests: (0..<failedRequests).map {
                RequestRecord(id: LogicalRequestID("failed-\($0)"), originalText: "question \($0)", phase: .failed)
            })
        presentation = PresentationSession(state: state)
        subject.send(state)
    }

    var lastEvent: SemanticEvent? { nil }
    var currentHeader: PresentedHeader { PresentedHeader(location: "Test", timeOfDay: "Morning", weather: "Clear") }
    var persistenceError: String? { nil }
    var statePublisher: AnyPublisher<SessionState, Never> { subject.eraseToAnyPublisher() }
    var initialFollowsNewest: Bool { true }
    var initialUnreadCount: Int { 0 }

    func start() {}
    func setInferenceAllowed(_ allowed: Bool) { inferenceAllowed = allowed }

    func updateDraft(_ text: String) {
        presentation.updateDraft(text)
        state = presentation.state
        subject.send(state)
    }

    func restoredDraft() -> Draft? { nil }
    func persistDraft(_ text: String) -> String? { nil }
    func persistSessionState() -> String? { nil }

    func persistLifecycleSnapshot() async {
        lifecycleEvents.append("persist")
    }

    func followNewest() {
        followNewestCalls += 1
        var viewport = state.viewport
        viewport.followNewest()
        state = replacingViewport(viewport)
        presentation = PresentationSession(state: state)
        subject.send(state)
    }
    func readHistory(anchor: TranscriptAnchor?) {}
    func loadOlderTranscript() async {}

    func submit(_ text: String) async throws -> SubmissionReceipt {
        submitCalls += 1
        submitStarted = true
        if gateSubmit {
            await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
                submitContinuation = continuation
            }
        }
        if acceptedRequestRemainsActive {
            let requestID = LogicalRequestID("request-new")
            state = SessionState(draft: state.draft, activeRequestID: requestID)
            subject.send(state)
        }
        return SubmissionReceipt(
            logicalRequestID: LogicalRequestID("request-new"),
            attemptID: ExecutionAttemptID("attempt-new"),
            commandEventID: nil,
            accepted: acceptedReceipt,
            isRetry: false,
            cursor: EventCursor(1)
        )
    }

    func setReadingHistory() {
        presentation.readHistory(anchor: TranscriptAnchor(itemID: TranscriptItemID("older"), offset: 0))
        state = presentation.state
        subject.send(state)
    }

    private func replacingViewport(_ viewport: TranscriptViewport) -> SessionState {
        SessionState(
            sessionID: state.sessionID,
            contractVersion: state.contractVersion,
            stateRevision: state.stateRevision,
            eventCursor: state.eventCursor,
            transcript: state.transcript,
            hasOlderTranscript: state.hasOlderTranscript,
            draft: state.draft,
            requests: state.requests,
            commandHistory: state.commandHistory,
            pendingClarification: state.pendingClarification,
            scene: state.scene,
            viewport: viewport,
            isHistoricalWindow: state.isHistoricalWindow,
            activeRequestID: state.activeRequestID,
            lastError: state.lastError,
            transcriptCapacity: state.transcriptCapacity,
            processedEventCapacity: state.processedEventCapacity,
            processedEventIDs: state.processedEventIDs,
            streamProgress: state.streamProgress
        )
    }

    func releaseSubmit() {
        gateSubmit = false
        submitContinuation?.resume()
        submitContinuation = nil
    }

    func stop() async throws -> StopReceipt {
        stopCalls += 1
        lifecycleEvents.append("stop")
        state = SessionState()
        subject.send(state)
        return StopReceipt(result: .cancelled)
    }

    func retryLastFailed() async throws {
        guard let request = state.requests.last(where: { $0.phase.canRetry }) else {
            throw FixtureAdapterError.requestNotRetryable
        }
        submitStarted = true
        if gateSubmit {
            await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
                submitContinuation = continuation
            }
        }
        state = SessionState(requests: state.requests, activeRequestID: request.id)
        subject.send(state)
    }
    func step() async -> FixtureStepResult { FixtureStepResult(event: nil, isFinished: true) }
    func answerClarification(choiceID: String) async throws { throw FixtureAdapterError.noClarificationPending }
    func suggestions(for text: String) -> [CompletionItem] { [] }
    func insert(_ item: CompletionItem, into text: String) -> String { text }
}
