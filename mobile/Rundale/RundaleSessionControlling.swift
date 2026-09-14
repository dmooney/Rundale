import Combine
import RundaleKit

/// Presentation-facing session boundary used by the SwiftUI model.
///
/// The fixture controller and the future embedded Parish controller both
/// expose the same state projection and command surface. Views therefore stay
/// independent of the source of semantic events and of the engine that
/// produces them.
@MainActor
protocol RundaleSessionControlling: AnyObject {
    var state: SessionState { get }
    var lastEvent: SemanticEvent? { get }
    var currentHeader: PresentedHeader { get }
    var persistenceError: String? { get }
    var statePublisher: AnyPublisher<SessionState, Never> { get }

    var initialFollowsNewest: Bool { get }
    var initialUnreadCount: Int { get }

    func start()
    func updateDraft(_ text: String)
    func restoredDraft() -> Draft?
    @discardableResult
    func persistDraft(_ text: String) -> String?
    @discardableResult
    func persistSessionState() -> String?
    func persistLifecycleSnapshot() async

    func followNewest()
    func readHistory(anchor: TranscriptAnchor?)

    func submit(_ text: String) async throws -> SubmissionReceipt
    func stop() async throws -> StopReceipt
    func retryLastFailed() async throws
    func step() async -> FixtureStepResult
    func answerClarification(choiceID: String) async throws

    func suggestions(for text: String) -> [CompletionItem]
    func insert(_ item: CompletionItem, into text: String) -> String
}
