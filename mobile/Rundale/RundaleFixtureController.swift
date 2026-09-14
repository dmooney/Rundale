import Combine
import Foundation
import RundaleKit

/// Main-actor presentation bridge for the fixture adapter. `FixtureSessionAdapter`
/// remains the fixture/session authority in RundaleKit; this type only feeds
/// its semantic events through the package reducer and exposes a SwiftUI-safe
/// snapshot.
@MainActor
final class RundaleFixtureController: ObservableObject, RundaleSessionControlling {
    @Published private(set) var state: SessionState
    @Published private(set) var lastEvent: SemanticEvent?

    let adapter: FixtureSessionAdapter
    let completionRegistry: FixtureCompletionRegistry
    let draftStore: FixtureDraftStore
    let sessionStore: FixtureSessionStore
    let manualStream: Bool
    let headerTimeOfDay: String
    let headerWeather: String
    @Published private(set) var persistenceError: String?

    var statePublisher: AnyPublisher<SessionState, Never> {
        $state.eraseToAnyPublisher()
    }

    var currentHeader: PresentedHeader {
        PresentedHeader(
            location: state.scene?.name ?? "The crossroads",
            timeOfDay: headerTimeOfDay,
            weather: headerWeather
        )
    }

    private let presentation: PresentationSession
    private let canPersistSession: Bool
    private let persistenceWriter: FixtureSessionSnapshotWriter
    /// Monotonically identifies each snapshot submission from this main-actor
    /// host. The writer uses this token rather than the event cursor because
    /// lifecycle and viewport saves can legitimately share a cursor.
    private var persistenceGeneration: UInt64 = 0
    private var eventTask: Task<Void, Never>?
    private var automaticTask: Task<Void, Never>?

    init(configuration: LaunchConfiguration) {
        let applicationSupport = FileManager.default
            .urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        let sessionURL: URL
        if let draftFileURL = configuration.draftFileURL {
            sessionURL = draftFileURL
                .deletingLastPathComponent()
                .appendingPathComponent("phase1-session.json")
        } else {
            sessionURL = applicationSupport.appendingPathComponent("Rundale/phase1-session.json")
        }
        let store = FixtureSessionStore(fileURL: sessionURL)
        persistenceWriter = FixtureSessionSnapshotWriter(store: store)
        var restoredState: SessionState?
        var restorationError: String?
        // A normal launch rehydrates the last local session. The restoration
        // fixtures only make this path deterministic for UI tests; they are
        // not a separate product mode.
        if !configuration.resetFixture {
            do {
                let saved = try store.restore()
                // The adapter's restoring initializer rehydrates the request
                // inputs and emits a single interrupted terminal event if a
                // save captured an active attempt. Keep the persisted state
                // intact so history, IDs, and the viewport survive relaunch.
                restoredState = saved
            } catch let error as FixturePersistenceError {
                if case .missingFile = error {
                    restoredState = nil
                } else {
                    restorationError = error.localizedDescription
                }
            } catch {
                restorationError = error.localizedDescription
            }
        }
        let script = Self.script(for: configuration.fixture)
        let sessionID = restoredState?.sessionID ?? SessionID()
        if let restoredState {
            adapter = FixtureSessionAdapter(script: script, restoring: restoredState)
        } else {
            adapter = FixtureSessionAdapter(sessionID: sessionID, script: script)
        }
        presentation = PresentationSession(state: restoredState ?? SessionState(sessionID: sessionID))
        state = presentation.state
        completionRegistry = .phase1
        let draftURL = configuration.draftFileURL
            ?? FileManager.default
                .urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
                .appendingPathComponent("Rundale/phase1-draft.json")
        draftStore = FixtureDraftStore(fileURL: draftURL)
        sessionStore = store
        if configuration.resetFixture {
            try? draftStore.remove()
            try? sessionStore.remove()
        }
        manualStream = configuration.manualStream
        headerTimeOfDay = "Late evening"
        headerWeather = "Rain easing"
        persistenceError = restorationError
        canPersistSession = restorationError == nil
    }

    deinit {
        eventTask?.cancel()
        automaticTask?.cancel()
    }

    func start() {
        guard eventTask == nil else { return }
        let adapter = self.adapter
        let cursor = state.eventCursor
        eventTask = Task { [weak self, adapter] in
            let stream = await adapter.events(after: cursor)
            do {
                for try await event in stream {
                    guard !Task.isCancelled else { return }
                    guard let owner = self else { return }
                    await owner.consumeAdapterEvent(event, persistBoundary: true)
                }
            } catch {
                guard !Task.isCancelled, let owner = self else { return }
                owner.persistenceError = error.localizedDescription

                // The adapter journal remains authoritative. Reconcile every
                // event after the last applied cursor before resubscribing so
                // the bounded stream can recover without dropping gameplay
                // facts that arrived while the UI was stalled.
                if error is FixtureAdapterError {
                    owner.eventTask = nil
                    _ = await owner.reconcilePendingAdapterEvents(persistBoundary: false)
                    await owner.persistSessionStateAndWait()
                    owner.start()
                }
            }
        }

    }

    private func beginAutomaticStepping() {
        guard automaticTask == nil, !manualStream else { return }
        let adapter = self.adapter
        automaticTask = Task { [weak self, adapter] in
            defer { self?.automaticTask = nil }
            while !Task.isCancelled {
                guard await adapter.isStreaming else { return }
                try? await Task.sleep(nanoseconds: 2_750_000_000)
                guard !Task.isCancelled, await adapter.isStreaming else { return }
                _ = await adapter.step()
            }
        }
    }

    func updateDraft(_ text: String) {
        presentation.updateDraft(text)
        state = presentation.state
    }

    var initialFollowsNewest: Bool { state.viewport.isFollowingNewest }
    var initialUnreadCount: Int { state.viewport.unreadCount }

    func followNewest() {
        presentation.followNewest()
        state = presentation.state
        persistSessionState()
    }

    func readHistory(anchor: TranscriptAnchor? = nil) {
        presentation.readHistory(anchor: anchor)
        state = presentation.state
        persistSessionState()
    }

    func restoredDraft() -> Draft? {
        try? draftStore.restore()
    }

    @discardableResult
    func persistDraft(_ text: String) -> String? {
        let draft = Draft(id: state.draft.id, text: text)
        do {
            try draftStore.save(draft)
            return nil
        } catch {
            persistenceError = error.localizedDescription
            return persistenceError
        }
    }

    @discardableResult
    func persistSessionState() -> String? {
        guard canPersistSession else { return persistenceError }
        let snapshot = state
        let generation = nextPersistenceGeneration()
        let writer = persistenceWriter
        Task { [weak self] in
            let error = await writer.save(state: snapshot, generation: generation)
            guard let self else { return }
            if let error {
                persistenceError = error
            }
        }
        return nil
    }

    /// Reconcile the adapter journal before taking a lifecycle snapshot. An
    /// adapter call emits before its AsyncStream subscriber necessarily runs,
    /// so saving the last published state directly could omit an accepted
    /// request at the exact moment the app becomes inactive.
    func persistLifecycleSnapshot() async {
        _ = await reconcilePendingAdapterEvents(persistBoundary: false)
        await persistSessionStateAndWait()
    }

    private func persistSessionStateAndWait() async {
        guard canPersistSession else { return }
        let snapshot = state
        let generation = nextPersistenceGeneration()
        let error = await persistenceWriter.save(state: snapshot, generation: generation)
        if let error {
            persistenceError = error
        }
    }

    private func nextPersistenceGeneration() -> UInt64 {
        defer { persistenceGeneration &+= 1 }
        return persistenceGeneration
    }

    func submit(_ text: String) async throws -> SubmissionReceipt {
        let draftID = presentation.state.draft.id
        let receipt = try await adapter.submit(text: text, draftID: draftID, logicalRequestID: nil)
        _ = await reconcilePendingAdapterEvents(persistBoundary: false)
        await persistSessionStateAndWait()
        beginAutomaticStepping()
        return receipt
    }

    func retryLastFailed() async throws {
        guard let request = state.requests.last(where: { $0.phase.canRetry }) else {
            throw FixtureAdapterError.requestNotRetryable
        }
        _ = try await adapter.retry(logicalRequestID: request.id)
        _ = await reconcilePendingAdapterEvents(persistBoundary: false)
        await persistSessionStateAndWait()
        beginAutomaticStepping()
    }

    func stop() async -> StopReceipt {
        let receipt = await adapter.stop()
        _ = await reconcilePendingAdapterEvents(persistBoundary: false)
        await persistSessionStateAndWait()
        return receipt
    }

    func step() async -> FixtureStepResult {
        let result = await adapter.step()
        _ = await reconcilePendingAdapterEvents(persistBoundary: false)
        if result.isFinished {
            await persistSessionStateAndWait()
        }
        return result
    }

    func answerClarification(choiceID: String) async throws {
        guard let pending = state.pendingClarification else {
            throw FixtureAdapterError.noClarificationPending
        }
        _ = try await adapter.answerClarification(
            logicalRequestID: pending.requestID,
            choiceID: choiceID
        )
        _ = await reconcilePendingAdapterEvents(persistBoundary: false)
        await persistSessionStateAndWait()
        beginAutomaticStepping()
    }

    func suggestions(for text: String) -> [CompletionItem] {
        completionRegistry.suggestions(for: text)
    }

    func insert(_ item: CompletionItem, into text: String) -> String {
        completionRegistry.applying(item, to: text)
    }

    /// Applies any adapter events not yet reflected in presentation state.
    /// The live AsyncStream and this receipt-time catch-up share the same
    /// reducer lane; duplicate delivery is therefore harmless and does not
    /// repeat UI notifications or persistence side effects.
    @discardableResult
    private func reconcilePendingAdapterEvents(persistBoundary: Bool) async -> Bool {
        let cursor = presentation.state.eventCursor
        let events = await adapter.allEvents()
        let pending = events.filter { event in
            event.sequence.rawValue > cursor.rawValue
        }
        var appliedAny = false
        for event in pending {
            let applied = await consumeAdapterEvent(event, persistBoundary: persistBoundary)
            appliedAny = appliedAny || applied
        }
        return appliedAny
    }

    @discardableResult
    private func consumeAdapterEvent(_ event: SemanticEvent,
                                     persistBoundary: Bool) async -> Bool {
        let previousState = presentation.state
        let result = presentation.apply(event)
        let applied = result == .applied

        if applied {
            lastEvent = event
        }
        if presentation.state != previousState {
            state = presentation.state
        }

        if applied, persistBoundary, isPersistenceBoundary(event) {
            await persistSessionStateAndWait()
        }
        return applied
    }

    private func isPersistenceBoundary(_ event: SemanticEvent) -> Bool {
        event.kind == .playerCommand
            || (event.kind == .progress && event.metadata["retry"] == "true")
            || presentation.state.activeRequestID == nil
    }

    private static func script(for fixture: LaunchConfiguration.Fixture) -> FixtureScript {
        switch fixture {
        case .longHistory:
            return .longHistory(count: 180)
        case .restorationLongHistory:
            return .longHistory(count: 180)
        case .standard, .manualStream, .failed, .interrupted, .clarification, .restoration, .rejected:
            return .phase1
        }
    }

}
