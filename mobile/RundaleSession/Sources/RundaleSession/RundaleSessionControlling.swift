import RundaleKit

/// Presentation-facing session boundary used by the presenter.
///
/// The fixture controller and the embedded Limerick controller both expose
/// the same state projection and command surface. Views therefore stay
/// independent of the source of semantic events and of the engine that
/// produces them.
@MainActor
public protocol RundaleSessionControlling: AnyObject {
    var state: SessionState { get }
    var lastEvent: SemanticEvent? { get }
    var currentHeader: PresentedHeader { get }
    var persistenceError: String? { get }
    /// Signals each state change, and each new persistence error, starting
    /// with one for the current state. Signals the consumer has not read yet
    /// collapse into one, and the consumer reads `state` itself.
    func stateChanges() -> AsyncStream<Void>

    var initialFollowsNewest: Bool { get }
    var initialUnreadCount: Int { get }

    func start()
    func setInferenceAllowed(_ allowed: Bool)
    func updateDraft(_ text: String)
    func restoredDraft() -> Draft?
    @discardableResult
    func persistDraft(_ text: String) -> String?
    @discardableResult
    func persistSessionState() -> String?
    func persistLifecycleSnapshot() async

    func followNewest()
    func readHistory(anchor: TranscriptAnchor?)
    func loadOlderTranscript() async

    func submit(_ text: String) async throws -> SubmissionReceipt
    func stop() async throws -> StopReceipt
    func retryLastFailed() async throws
    func step() async -> FixtureStepResult
    func answerClarification(choiceID: String) async throws
    /// The plain-text report `/bug` sends to `limerick-bug-report`.
    /// Reads only: it never becomes a request and changes no state.
    func bugReport(description: String) async throws -> String

    func suggestions(for text: String) -> [CompletionItem]
    /// The short command list the Commands button offers; typing `/`
    /// offers every command through `suggestions(for:)`.
    var advertisedCommands: [CompletionItem] { get }
    func insert(_ item: CompletionItem, into text: String) -> String
}

/// Fans `RundaleSessionControlling.stateChanges()` out to each subscriber.
/// Like a `@Published` value read through `.values`, a new subscriber gets
/// one signal at once, and signals it has not read yet collapse into one.
@MainActor
public final class SessionChangeSignal {
    private var continuations: [Int: AsyncStream<Void>.Continuation] = [:]
    private var nextID = 0

    public init() {}

    public func stream() -> AsyncStream<Void> {
        let (stream, continuation) = AsyncStream<Void>.makeStream(bufferingPolicy: .bufferingNewest(1))
        let id = nextID
        nextID += 1
        continuations[id] = continuation
        continuation.onTermination = { @Sendable [weak self] _ in
            Task { @MainActor in self?.continuations.removeValue(forKey: id) }
        }
        continuation.yield()
        return stream
    }

    public func send() {
        for continuation in continuations.values {
            continuation.yield()
        }
    }

    deinit {
        for continuation in continuations.values {
            continuation.finish()
        }
    }
}
