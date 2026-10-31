import Foundation
import RundaleKit
import XCTest
@testable import RundaleSession

@MainActor
final class PresenterDispatchTests: XCTestCase {
    func testIdleStateNotificationCannotClearStreamingWhileSubmitReceiptIsPending() async {
        let session = SuspendedSubmitSession()
        let launch = LaunchConfiguration(arguments: ["--phase3"], environment: [:], bundle: [:])
        let presenter = RundaleSessionPresenter(
            launch: launch,
            session: session,
            bugReports: nil,
            captureScreenshot: { nil },
            deviceDescription: "test"
        )
        var refreshesWhileDispatching = 0
        presenter.willChange = {
            if presenter.isDispatching { refreshesWhileDispatching += 1 }
        }
        presenter.start()
        presenter.draft = "go to Connolly Cottage"
        presenter.noteDraftMutation()
        presenter.submitDraft()

        await waitUntil { session.submitStarted }
        // beginControl publishes once. The fake then emits an idle snapshot
        // notification while submit is suspended, which must not clear Stop.
        await waitUntil { refreshesWhileDispatching >= 2 }
        XCTAssertTrue(presenter.isStreaming)
        XCTAssertTrue(presenter.showsStop)
        XCTAssertFalse(presenter.showsRetry)

        session.finishSubmit()
        await waitUntil { !presenter.isDispatching && !presenter.isStreaming }
        XCTAssertFalse(presenter.showsStop)
    }
}

@MainActor
private final class SuspendedSubmitSession: RundaleSessionControlling {
    private let changes = SessionChangeSignal()
    private var continuation: CheckedContinuation<SubmissionReceipt, Error>?
    private(set) var submitStarted = false

    var state = SessionState()
    var lastEvent: SemanticEvent?
    var currentHeader = PresentedHeader(location: "Connolly Cottage", timeOfDay: "Morning", weather: "Clear")
    var persistenceError: String?
    var initialFollowsNewest = true
    var initialUnreadCount = 0

    func stateChanges() -> AsyncStream<Void> { changes.stream() }
    func start() {}
    func newGame() async throws {}
    func setInferenceAllowed(_ allowed: Bool) {}
    func updateDraft(_ text: String) {}
    func restoredDraft() -> Draft? { nil }
    func persistDraft(_ text: String) -> String? { nil }
    func persistSessionState() -> String? { nil }
    func persistLifecycleSnapshot() async {}
    func followNewest() {}
    func readHistory(anchor: TranscriptAnchor?) {}
    func loadOlderTranscript() async {}

    func submit(_ text: String) async throws -> SubmissionReceipt {
        submitStarted = true
        return try await withCheckedThrowingContinuation { continuation in
            self.continuation = continuation
            // This is the problematic controller ordering: publish the still
            // idle state before the command's receipt has returned.
            changes.send()
        }
    }

    func stop() async throws -> StopReceipt { StopReceipt(result: .noActiveRequest) }
    func retryLastFailed() async throws {}
    func step() async -> FixtureStepResult { FixtureStepResult(event: nil) }
    func answerClarification(choiceID: String) async throws {}
    func bugReport(description: String) async throws -> String { "" }
    func suggestions(for text: String) -> [CompletionItem] { [] }
    var advertisedCommands: [CompletionItem] { [] }
    func insert(_ item: CompletionItem, into text: String) -> String { text }

    func finishSubmit() {
        continuation?.resume(returning: SubmissionReceipt(
            logicalRequestID: LogicalRequestID("request"),
            attemptID: ExecutionAttemptID("attempt"),
            accepted: true,
            cursor: EventCursor(0)
        ))
        continuation = nil
    }
}
