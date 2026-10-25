import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import LimerickEndpointKit
import RundaleBridge
import RundaleKit

/// The attempt id one Endpoint call sends. The server keys a stream by
/// request and attempt, and one line can call several speakers in one
/// attempt, so each call sends its own id: the call id "{attempt}#{n}",
/// with '#' (not a valid correlation character) made '.'.
private func endpointAttemptID(callID: String) -> String {
    callID.replacingOccurrences(of: "#", with: ".")
}

/// The native application adapter. Rust owns the game/session state and its
/// SQLite transaction boundaries; this main-actor object only projects Rust
/// semantic events for the presenter and owns the platform Endpoint stream.
@MainActor
public final class RundaleEngineController: RundaleSessionControlling {
    public private(set) var state: SessionState {
        didSet { changes.send() }
    }

    public private(set) var lastEvent: SemanticEvent?
    /// Also signals `stateChanges()`: an error can arrive without a state
    /// change (opening the engine fails, for example), and the presenter only
    /// reads `persistenceError` when it is signalled.
    public private(set) var persistenceError: String? {
        didSet { changes.send() }
    }

    /// Retained locally for diagnostics; never rendered or sent to inference.
    public private(set) var persistenceDiagnostic: String?

    private let configuration: LaunchConfiguration
    private let projectionStore: Phase2ProjectionStore
    private let endpointClient: LimerickEndpointClient
    private let openRuntime: @Sendable (Data) throws -> any EngineRuntime
    /// Records each Endpoint failure for diagnosis. The text holds no
    /// credentials and is never shown to the player.
    private let log: @Sendable (String) -> Void
    private let changes = SessionChangeSignal()
    private var completionRegistry: FixtureCompletionRegistry
    private var presentation: PresentationSession
    private var runtime: (any EngineRuntime)?
    private var bootstrapTask: Task<Void, Never>?
    private var eventTask: Task<Void, Never>?
    private var endpointTask: Task<Void, Never>?
    private var activeEndpointRequest: EndpointRequest?
    private var didStart = false
    private var didHydrateRuntime = false
    private var allowsInference = true
    private var inferenceGeneration: UInt64 = 0
    private var historyExhausted = false
    private var historyWindowShifted = false
    private var historyBeforeCursor: EventCursor?
    private let restoredHistoryCursor: EventCursor?
    private let restoredSessionID: SessionID?
    private var engineTimeOfDay = "Morning"
    private var engineWeather = "Clear"

    public func stateChanges() -> AsyncStream<Void> {
        changes.stream()
    }

    public var currentHeader: PresentedHeader {
        PresentedHeader(
            location: state.scene?.name ?? "Kilteevan Village",
            timeOfDay: engineTimeOfDay,
            weather: engineWeather
        )
    }

    public var initialFollowsNewest: Bool { state.viewport.isFollowingNewest }
    public var initialUnreadCount: Int { state.viewport.unreadCount }

    /// - Parameters:
    ///   - credentials: the Endpoint credentials of a live launch; not made
    ///     for a mock-transport launch.
    ///   - modDirectory: the world the engine plays, the canonical mod.
    ///   - log: where Endpoint failures are recorded.
    ///   - transport: carries a live launch's Endpoint calls; `nil` for
    ///     URLSession.
    ///   - openRuntime: opens the engine from its resume payload.
    public init(
        configuration: LaunchConfiguration,
        credentials: () -> any LimerickEndpointKit.EndpointCredentialProvider,
        modDirectory: URL?,
        log: @escaping @Sendable (String) -> Void,
        transport: (any EndpointTransport)? = nil,
        openRuntime: @escaping @Sendable (Data) throws -> any EngineRuntime = {
            try LimerickRuntime.openResume(payload: $0)
        }
    ) {
        self.configuration = configuration
        self.log = log
        self.openRuntime = openRuntime
        projectionStore = Phase2ProjectionStore(configuration: configuration, modDirectory: modDirectory)

        if configuration.resetFixture {
            projectionStore.remove()
        }

        let projection = projectionStore.restore()
        restoredHistoryCursor = projection?.anchorEventCursor
        restoredSessionID = projection?.sessionID
        let initialState = SessionState(
            draft: projection?.draft ?? Draft(text: configuration.initialDraft ?? ""),
            viewport: projection?.viewport ?? TranscriptViewport()
        )
        state = initialState
        presentation = PresentationSession(state: initialState)
        // The Rust read model is not available until the runtime opens. The
        // first snapshot refresh replaces this empty projection with the
        // authoritative nearby-person list.
        completionRegistry = FixtureCompletionRegistry(commands: [], nearbyNPCs: [])

        if configuration.phase2MockTransport {
            let credentials = StaticEndpointCredentialProvider(
                LimerickEndpointKit.EndpointCredentials(
                    authorizationToken: "phase2-ui-test",
                    appCheckToken: "phase2-ui-test"
                )
            )
            endpointClient = LimerickEndpointClient(
                credentials: credentials,
                transport: Phase2MockEndpointTransport(),
                policy: EndpointURLPolicy(allowLoopbackHTTP: true)
            )
        } else if let transport {
            endpointClient = LimerickEndpointClient(credentials: credentials(), transport: transport)
        } else {
            endpointClient = LimerickEndpointClient(credentials: credentials())
        }

        if !configuration.resetFixture, let error = projectionStore.restoreError {
            persistenceError = error
        }
    }

    deinit {
        bootstrapTask?.cancel()
        eventTask?.cancel()
        endpointTask?.cancel()
    }

    public func start() {
        guard !didStart else { return }
        didStart = true
        let openPayload = projectionStore.engineOpenPayload
        let saveDirectory = projectionStore.engineURL.deletingLastPathComponent()
        let openRuntime = self.openRuntime
        bootstrapTask = Task { [weak self] in
            do {
                let runtime = try await Task.detached(priority: nil) {
                    // SQLite canonicalizes its parent before opening. Prepare
                    // it before bootstrap, not as a side effect of typing a draft.
                    try FileManager.default.createDirectory(at: saveDirectory, withIntermediateDirectories: true)
                    return try openRuntime(openPayload)
                }.value
                guard let self else {
                    try? await runtime.close()
                    return
                }
                self.runtime = runtime
                let data = try await runtime.snapshotJSON()
                let snapshot = try FixtureJSON.decode(EngineSnapshot.self, from: data)
                var historyPage: LimerickEventPage?
                if !self.state.viewport.isFollowingNewest,
                   self.restoredSessionID == snapshot.sessionID,
                   let cursor = self.restoredHistoryCursor,
                   cursor.rawValue < UInt64.max {
                    historyPage = try await runtime.readEventPageBefore(
                        before: EventCursor(cursor.rawValue + 1), limit: 100
                    )
                }
                try self.refreshFromSnapshot(data, restoredHistoryPage: historyPage)
                self.startEventSubscription(runtime: runtime)
                guard self.allowsInference else { return }
                if let invocation = try await self.pendingInvocation(runtime: runtime),
                   self.allowsInference {
                    self.startEndpoint(invocation, runtime: runtime)
                }
            } catch {
                guard let self else { return }
                self.persistenceDiagnostic = String(reflecting: error)
                self.persistenceError = Self.playerFacingPersistenceError(error)
            }
        }
    }

    public func setInferenceAllowed(_ allowed: Bool) {
        allowsInference = allowed
        inferenceGeneration &+= 1
        guard !allowed else {
            guard let runtime else { return }
            let generation = inferenceGeneration
            Task { [weak self, runtime] in
                guard let self, self.allowsInference, self.inferenceGeneration == generation else { return }
                do {
                    if let invocation = try await self.pendingInvocation(runtime: runtime),
                       self.allowsInference,
                       self.inferenceGeneration == generation {
                        self.startEndpoint(invocation, runtime: runtime)
                    }
                } catch {
                    self.persistenceError = Self.playerFacingPersistenceError(error)
                }
            }
            return
        }
        endpointTask?.cancel()
        endpointTask = nil
    }

    public func updateDraft(_ text: String) {
        presentation.updateDraft(text)
        state = presentation.state
    }

    public func restoredDraft() -> Draft? {
        projectionStore.restore()?.draft
    }

    @discardableResult
    public func persistDraft(_ text: String) -> String? {
        var draft = state.draft
        draft = Draft(id: draft.id, text: text)
        return persistProjection(draft: draft, viewport: state.viewport)
    }

    @discardableResult
    public func persistSessionState() -> String? {
        persistProjection(draft: state.draft, viewport: state.viewport)
    }

    public func persistLifecycleSnapshot() async {
        // Rust commits acceptance, terminal transitions, and gameplay state in
        // its SQLite store. Awaiting one snapshot call drains the actor lane
        // before the separate Swift presentation projection is written.
        if let runtime {
            do {
                _ = try await runtime.snapshotJSON()
            } catch {
                persistenceError = Self.playerFacingPersistenceError(error)
            }
        }
        _ = persistSessionState()
    }

    public func followNewest() {
        presentation.followNewest()
        state = presentation.state
        _ = persistSessionState()
        guard historyWindowShifted, let runtime else { return }
        Task { [weak self, runtime] in
            do {
                let data = try await runtime.snapshotJSON()
                guard let self else { return }
                try self.refreshFromSnapshot(data)
                self.historyWindowShifted = false
            } catch {
                self?.persistenceError = Self.playerFacingPersistenceError(error)
            }
        }
    }

    public func readHistory(anchor: TranscriptAnchor? = nil) {
        presentation.readHistory(anchor: anchor)
        state = presentation.state
        _ = persistSessionState()
    }

    /// Loads one bounded page immediately before the retained transcript edge.
    /// The backward cursor is independent from the live subscription cursor,
    /// so a long history does not require replaying the save from sequence 0.
    public func loadOlderTranscript() async {
        guard let runtime,
              state.hasOlderTranscript,
              !historyExhausted else { return }
        guard let oldestSequence = state.transcript.first?.lastEventSequence.rawValue,
              oldestSequence > 0 else {
            historyExhausted = true
            presentation.loadOlderTranscript(items: [], hasOlderItems: false)
            state = presentation.state
            return
        }
        do {
            let page = try await runtime.readEventPageBefore(
                before: historyBeforeCursor ?? EventCursor(oldestSequence),
                limit: 100
            )
            let olderEvents = page.events.filter { $0.sequence.rawValue < oldestSequence }
            guard !olderEvents.isEmpty else {
                historyExhausted = true
                presentation.loadOlderTranscript(items: [], hasOlderItems: false)
                state = presentation.state
                return
            }
            historyBeforeCursor = page.nextCursor
            let items = Self.transcriptItems(
                from: olderEvents,
                sessionID: state.sessionID,
                capacity: state.transcriptCapacity
            )
            presentation.loadOlderTranscript(items: items, hasOlderItems: page.hasMore)
            state = presentation.state
            historyWindowShifted = true
            _ = persistSessionState()
        } catch {
            persistenceError = "Older transcript could not be loaded. Try again."
        }
    }

    public func submit(_ text: String) async throws -> SubmissionReceipt {
        guard let runtime else { throw LimerickRuntimeError.closed }
        let receipt = try await runtime.submit(
            text: text,
            draftID: state.draft.id,
            logicalRequestID: nil
        )
        try refreshFromSnapshot(try await runtime.snapshotJSON())
        _ = persistSessionState()
        if let invocation = try await pendingInvocation(runtime: runtime) {
            startEndpoint(invocation, runtime: runtime)
        }
        return receipt
    }

    public func stop() async throws -> StopReceipt {
        let cancellationRequest = activeEndpointRequest
        endpointTask?.cancel()
        endpointTask = nil
        activeEndpointRequest = nil
        let cancellationTask = cancellationRequest.map { request in
            Task { [endpointClient] in try? await endpointClient.cancel(request) }
        }
        guard let runtime else { return StopReceipt(result: .noActiveRequest) }
        let receipt = try await runtime.stop()
        try? refreshFromSnapshot(try await runtime.snapshotJSON())
        _ = persistSessionState()
        _ = await cancellationTask?.value
        return receipt
    }

    public func retryLastFailed() async throws {
        guard let runtime else { throw LimerickRuntimeError.closed }
        guard let request = state.requests.last(where: { $0.phase.canRetry }) else {
            throw FixtureAdapterError.requestNotRetryable
        }
        let receipt = try await runtime.retry(logicalRequestID: request.id)
        try refreshFromSnapshot(try await runtime.snapshotJSON())
        _ = persistSessionState()
        if receipt.accepted, let invocation = try await pendingInvocation(runtime: runtime) {
            startEndpoint(invocation, runtime: runtime)
        }
    }

    public func step() async -> FixtureStepResult {
        FixtureStepResult(event: nil, isFinished: true)
    }

    public func answerClarification(choiceID: String) async throws {
        guard let runtime else { throw LimerickRuntimeError.closed }
        guard let request = state.requests.last(where: { $0.phase == .awaitingClarification }) else {
            throw FixtureAdapterError.noClarificationPending
        }
        let receipt = try await runtime.answerClarification(
            logicalRequestID: request.id,
            choiceID: choiceID
        )
        try refreshFromSnapshot(try await runtime.snapshotJSON())
        _ = persistSessionState()
        if receipt.accepted, let invocation = try await pendingInvocation(runtime: runtime) {
            startEndpoint(invocation, runtime: runtime)
        }
    }

    public func bugReport(description: String) async throws -> String {
        guard let runtime else { throw LimerickRuntimeError.closed }
        return try await runtime.bugReport(description: description,
                                           build: configuration.buildDescription)
    }

    public func suggestions(for text: String) -> [CompletionItem] {
        completionRegistry.suggestions(for: text)
    }

    public var advertisedCommands: [CompletionItem] { completionRegistry.advertisedCommands }

    public func insert(_ item: CompletionItem, into text: String) -> String {
        completionRegistry.applying(item, to: text)
    }

    private func startEventSubscription(runtime: any EngineRuntime) {
        eventTask?.cancel()
        let cursor = state.eventCursor
        eventTask = Task { [weak self, runtime] in
            let stream = await runtime.events(after: cursor)
            do {
                for try await event in stream {
                    guard !Task.isCancelled else { return }
                    guard let self else { return }
                    self.consume(event)
                }
            } catch {
                guard !Task.isCancelled, let self else { return }
                self.persistenceError = Self.playerFacingPersistenceError(error)
                do {
                    // The runtime journal is authoritative. Rebuild the
                    // presentation from its latest snapshot, then subscribe
                    // from the refreshed cursor so an overflow cannot leave
                    // the UI permanently behind the engine.
                    try self.refreshFromSnapshot(try await runtime.snapshotJSON())
                    self.startEventSubscription(runtime: runtime)
                } catch {
                    self.persistenceError = Self.playerFacingPersistenceError(error)
                }
            }
        }
    }

    private func consume(_ event: SemanticEvent) {
        let result = presentation.apply(event)
        if result == .applied { lastEvent = event }
        state = presentation.state
    }

    private static let bugCommand = SlashCompletionWord(
        word: RundaleSessionPresenter.bugCommandWord,
        summary: "Report a bug"
    )

    private func refreshFromSnapshot(_ data: Data, restoredHistoryPage: LimerickEventPage? = nil) throws {
        let snapshot = try FixtureJSON.decode(EngineSnapshot.self, from: data)
        engineTimeOfDay = snapshot.readModel.timeOfDay
        engineWeather = snapshot.readModel.weather
        // The engine's command registry: every command the phone runs and
        // what may follow each, the advertised short list, and everyone a
        // `/debug` name may complete to.
        // `/bug` is the app's, not the engine's: it reports, and never
        // reaches the turn pipeline.
        let appCommands = configuration.allowsBugReports ? [Self.bugCommand] : []
        completionRegistry = FixtureCompletionRegistry(
            commands: snapshot.readModel.commandCompletions + appCommands,
            advertised: snapshot.readModel.commands.map(\.name),
            nearbyNPCs: snapshot.readModel.nearbyPeople.map {
                FixtureNPCReference(id: $0.id, displayName: $0.displayName)
            },
            everyone: snapshot.readModel.everyone.map {
                FixtureNPCReference(id: $0.id, displayName: $0.name)
            }
        )

        // The first snapshot is a materialized Rust read model plus a bounded
        // event tail. Replay the tail into an empty presentation state, then
        // overlay the authoritative request/read-model metadata. This keeps
        // stable transcript IDs while avoiding the old bug where a tail that
        // no longer contained its PlayerCommand could not reconstruct the
        // request or command history.
        if !didHydrateRuntime || state.sessionID != snapshot.sessionID {
            hydrateFromSnapshot(snapshot, restoredHistoryPage: restoredHistoryPage)
            didHydrateRuntime = true
            return
        }

        // A history window intentionally ignores new transcript rows while
        // it is anchored near the older edge. Once the player follows newest,
        // rebuild the bounded tail from the authoritative snapshot instead of
        // trying to stitch a live suffix onto that older window.
        if historyWindowShifted, state.viewport.isFollowingNewest {
            hydrateFromSnapshot(snapshot)
            didHydrateRuntime = true
            return
        }

        // Later snapshots reconcile metadata and only apply events newer than
        // the current presentation cursor. Preserve provisional rows that are
        // live in the UI but are intentionally absent from Rust's durable
        // snapshot event tail.
        for event in snapshot.events.sorted(by: { $0.sequence < $1.sequence }) {
            consume(event)
        }
        let current = presentation.state
        let reconciled = SessionState(
            sessionID: snapshot.sessionID,
            contractVersion: snapshot.contractVersion,
            stateRevision: snapshot.stateRevision,
            eventCursor: snapshot.eventCursor,
            transcript: current.transcript,
            hasOlderTranscript: current.hasOlderTranscript || snapshot.hasOlderEvents,
            draft: current.draft,
            requests: snapshot.requests,
            commandHistory: commandHistory(for: snapshot.requests),
            pendingClarification: Self.pendingClarification(in: snapshot.requests),
            scene: snapshot.readModel.scene.summary,
            viewport: current.viewport,
            isHistoricalWindow: current.isHistoricalWindow,
            activeRequestID: snapshot.activeRequestID,
            lastError: current.lastError,
            transcriptCapacity: current.transcriptCapacity,
            processedEventCapacity: current.processedEventCapacity,
            processedEventIDs: current.processedEventIDs,
            streamProgress: current.streamProgress
        )
        presentation = PresentationSession(state: reconciled)
        state = reconciled
    }

    private func hydrateFromSnapshot(_ snapshot: EngineSnapshot, restoredHistoryPage: LimerickEventPage? = nil) {
        historyExhausted = !snapshot.hasOlderEvents
        historyWindowShifted = false
        historyBeforeCursor = nil
        let replay = PresentationSession(state: SessionState(
            sessionID: snapshot.sessionID,
            contractVersion: snapshot.contractVersion,
            draft: state.draft
        ))
        for event in snapshot.events.sorted(by: { $0.sequence < $1.sequence }) {
            replay.apply(event)
        }
        let replayed = replay.state
        var transcript = replayed.transcript
        var viewport = state.viewport
        if !viewport.isFollowingNewest, let anchor = viewport.anchor {
            if let page = restoredHistoryPage {
                let older = Self.transcriptItems(from: page.events, sessionID: snapshot.sessionID,
                                                 capacity: replayed.transcriptCapacity)
                if older.contains(where: { $0.id == anchor.itemID }) {
                    transcript = older
                    historyWindowShifted = true
                    historyBeforeCursor = page.nextCursor
                    historyExhausted = !page.hasMore
                }
            }
            if !transcript.contains(where: { $0.id == anchor.itemID }) {
                // Older projections have no durable cursor. Fall back to the
                // current tail explicitly instead of retaining a dangling lock.
                viewport.followNewest()
            }
        }
        let hydrated = SessionState(
            sessionID: snapshot.sessionID,
            contractVersion: snapshot.contractVersion,
            stateRevision: snapshot.stateRevision,
            eventCursor: snapshot.eventCursor,
            transcript: transcript,
            hasOlderTranscript: replayed.hasOlderTranscript || snapshot.hasOlderEvents,
            draft: state.draft,
            requests: snapshot.requests,
            commandHistory: commandHistory(for: snapshot.requests),
            pendingClarification: Self.pendingClarification(in: snapshot.requests),
            scene: snapshot.readModel.scene.summary,
            viewport: viewport,
            isHistoricalWindow: historyWindowShifted,
            activeRequestID: snapshot.activeRequestID,
            lastError: replayed.lastError,
            transcriptCapacity: replayed.transcriptCapacity,
            processedEventCapacity: replayed.processedEventCapacity,
            processedEventIDs: replayed.processedEventIDs,
            streamProgress: replayed.streamProgress
        )
        presentation = PresentationSession(state: hydrated)
        state = hydrated
    }

    private static func transcriptItems(
        from events: [SemanticEvent],
        sessionID: SessionID,
        capacity: Int
    ) -> [TranscriptItem] {
        let replay = PresentationSession(state: SessionState(
            sessionID: sessionID,
            transcriptCapacity: capacity
        ))
        for event in events.sorted(by: { $0.sequence < $1.sequence }) {
            replay.apply(event)
        }
        return replay.state.transcript
    }

    private static func pendingClarification(in requests: [RequestRecord]) -> PendingClarification? {
        guard let request = requests.last(where: { $0.phase == .awaitingClarification }),
              let attemptID = request.currentAttemptID,
              let prompt = request.pendingClarification else { return nil }
        return PendingClarification(requestID: request.id, attemptID: attemptID, prompt: prompt)
    }

    private func commandHistory(for requests: [RequestRecord]) -> [CommandHistoryEntry] {
        requests.enumerated()
            .sorted { lhs, rhs in
                let left = lhs.element.attempts.first?.startedAt.rawValue ?? UInt64.max
                let right = rhs.element.attempts.first?.startedAt.rawValue ?? UInt64.max
                return left == right ? lhs.offset < rhs.offset : left < right
            }
            .map { request in
                CommandHistoryEntry(
                    id: request.element.id,
                    text: request.element.originalText,
                    commandItemID: request.element.acceptedCommandItemID
                )
            }
    }

    private func pendingInvocation(runtime: any EngineRuntime) async throws -> LimerickPendingInvocation? {
        try await runtime.pendingInvocation()
    }

    /// Fulfils one model call through the Limerick Endpoint it names. Dialogue
    /// text deltas are shown provisionally; the terminal output (or the
    /// failure) resumes the engine, which may then ask for the next call of
    /// the same turn (intent, then dialogue).
    private func startEndpoint(_ invocation: LimerickPendingInvocation, runtime: any EngineRuntime) {
        endpointTask?.cancel()
        let configuration = self.configuration
        let endpointClient = self.endpointClient
        let generation = inferenceGeneration
        endpointTask = Task { [weak self, runtime] in
            do {
                guard let self,
                      self.allowsInference,
                      self.inferenceGeneration == generation,
                      !Task.isCancelled else { return }
                let body = try invocation.requestBody()
                let mockURL = "http://127.0.0.1/mock/\(invocation.slug)/versions/\(invocation.version)"
                    + (invocation.streams ? "/stream" : "")
                guard let endpointURL = configuration.endpointURL(
                    slug: invocation.slug, version: invocation.version, stream: invocation.streams
                ) ?? (configuration.phase2MockTransport ? URL(string: mockURL) : nil) else {
                    throw LimerickEndpointError.invalidURL
                }
                let request = try EndpointRequest(
                    url: endpointURL,
                    requestID: invocation.logicalRequestID.rawValue,
                    attemptID: endpointAttemptID(callID: invocation.callID),
                    idempotencyKey: invocation.callID,
                    endpointVersion: invocation.version,
                    policy: configuration.phase2MockTransport ? EndpointURLPolicy(allowLoopbackHTTP: true) : EndpointURLPolicy(),
                    body: body
                )
                guard self.allowsInference,
                      self.inferenceGeneration == generation,
                      !Task.isCancelled else { return }
                // An Endpoint that does not stream (intent) answers with its
                // output object on the JSON route. Stop cancels the task,
                // which cancels the request.
                guard invocation.streams else {
                    let completed = try await endpointClient.complete(request)
                    try Task.checkCancellation()
                    let response = try await runtime.resolve(invocation, output: completed.body)
                    guard !Task.isCancelled else { return }
                    self.consumeOperation(response)
                    try await self.continueInference(runtime: runtime, generation: generation)
                    return
                }
                // A stream is also cancelled server-side on Stop.
                self.activeEndpointRequest = request
                defer {
                    if self.activeEndpointRequest?.attemptID == request.attemptID {
                        self.activeEndpointRequest = nil
                    }
                }
                for try await frame in endpointClient.stream(request) {
                    try Task.checkCancellation()
                    switch frame.kind {
                    case .progress:
                        continue
                    case .textDelta:
                        guard invocation.isDialogue, let text = frame.text, !text.isEmpty else { continue }
                        let response = try await runtime.frame(invocation, sequence: frame.sequence, text: text)
                        guard !Task.isCancelled else { return }
                        self.consumeOperation(response)
                    case .final:
                        guard let payload = frame.payload else {
                            throw LimerickEndpointError.malformedEvent("final output is missing")
                        }
                        let response = try await runtime.resolve(invocation, output: payload)
                        guard !Task.isCancelled else { return }
                        self.consumeOperation(response)
                        try await self.continueInference(runtime: runtime, generation: generation)
                        return
                    case .error:
                        throw EndpointReportedFailure(payload: frame.error)
                    }
                }
            } catch is CancellationError {
                return
            } catch {
                guard !Task.isCancelled,
                      let self,
                      self.allowsInference,
                      self.inferenceGeneration == generation else { return }
                do {
                    let failure = Self.playerFacingFailure(error)
                    // The engine absorbs only a garbled intent answer (the
                    // turn continues as unclassified input), so record every
                    // Endpoint failure here for diagnosis. No credentials
                    // are part of these errors.
                    self.log("""
                    Endpoint \(invocation.slug) v\(invocation.version) \
                    failed (\(failure.reason.rawValue)): \(String(describing: error))
                    """)
                    let response = try await runtime.fail(
                        invocation, kind: failure.kind, message: failure.diagnostic, reason: failure.reason
                    )
                    self.consumeOperation(response)
                    try await self.continueInference(runtime: runtime, generation: generation)
                } catch {
                    self.persistenceError = Self.playerFacingPersistenceError(error)
                }
            }
        }
    }

    /// Refreshes the read model after the engine took a result and starts
    /// the next call the same turn is waiting on, if any.
    private func continueInference(runtime: any EngineRuntime, generation: UInt64) async throws {
        try refreshFromSnapshot(try await runtime.snapshotJSON())
        _ = persistSessionState()
        guard allowsInference, inferenceGeneration == generation else { return }
        if let next = try await pendingInvocation(runtime: runtime) {
            startEndpoint(next, runtime: runtime)
        }
    }

    private func consumeOperation(_ data: Data) {
        guard let result = try? FixtureJSON.decode(MobileOperationResult.self, from: data) else {
            return
        }
        for event in result.events.sorted(by: { $0.sequence < $1.sequence }) {
            consume(event)
        }
        _ = persistSessionState()
    }

    public static func playerFacingPersistenceError(_ error: Error) -> String {
        if case let .rejected(code, message)? = error as? LimerickRuntimeError,
           ["save_incompatible", "save_locked", "content_unavailable"].contains(code) {
            return message
        }
        if let projectionError = error as? Phase2ProjectionStoreError,
           let description = projectionError.errorDescription {
            return description
        }
        return "The saved game could not be updated. Your current game remains available; try again."
    }

    private func persistProjection(draft: Draft, viewport: TranscriptViewport) -> String? {
        do {
            let cursor = viewport.anchor.flatMap { anchor in
                state.transcript.first(where: { $0.id == anchor.itemID })
                    .map { EventCursor($0.lastEventSequence.rawValue) }
            }
            try projectionStore.save(Phase2Projection(draft: draft, viewport: viewport,
                                                      sessionID: state.sessionID, anchorEventCursor: cursor))
            return nil
        } catch {
            persistenceError = Self.playerFacingPersistenceError(error)
            return persistenceError
        }
    }
}

private struct EngineSnapshot: Decodable {
    let contractVersion: PresentationContractVersion
    let sessionID: SessionID
    let stateRevision: StateRevision
    let eventCursor: EventCursor
    let readModel: EngineReadModel
    let requests: [RequestRecord]
    let activeRequestID: LogicalRequestID?
    let events: [SemanticEvent]
    let hasOlderEvents: Bool
}

private struct EngineReadModel: Decodable {
    let scene: EngineScene
    let nearbyPeople: [EngineNearbyPerson]
    let timeOfDay: String
    let weather: String
    /// The slash commands `/help` and the Commands list offer.
    let commands: [EngineCommand]
    /// Every command the phone runs, with the words that may follow each.
    let commandCompletions: [SlashCompletionWord]
    /// Everyone in the world, by name, for `/debug` names.
    let everyone: [EngineWorldPerson]
}

private struct EngineWorldPerson: Decodable {
    let id: String
    let name: String
}

private struct EngineCommand: Decodable {
    let name: String
    let summary: String
}

private struct EngineScene: Decodable {
    let id: String
    let name: String
    let detail: String?

    var summary: SceneSummary {
        SceneSummary(id: id, name: name, detail: detail)
    }
}

private struct EngineNearbyPerson: Decodable {
    let id: String
    let displayName: String
}

private struct MobileOperationResult: Decodable {
    let events: [SemanticEvent]
}

private struct Phase2Projection: Codable, Sendable {
    let draft: Draft
    let viewport: TranscriptViewport
    let sessionID: SessionID?
    let anchorEventCursor: EventCursor?
}

public struct PlayerFacingFailure: Sendable {
    public let kind: LimerickRuntimeFailureKind
    /// Why it failed, as the player hears it (the mod's line for it).
    public let reason: LimerickFailureReason
    /// For the log only.
    public let diagnostic: String

    public init(_ kind: LimerickRuntimeFailureKind, _ reason: LimerickFailureReason, _ diagnostic: String) {
        self.kind = kind
        self.reason = reason
        self.diagnostic = diagnostic
    }
}

/// Pure, so callable from any isolation.
nonisolated extension RundaleEngineController {
    /// Why a model call failed: the reason the engine shows the mod's line
    /// for, and a diagnostic for the log (never shown to the player).
    static func playerFacingFailure(_ error: Error) -> PlayerFacingFailure {
        if let reported = error as? EndpointReportedFailure {
            return PlayerFacingFailure.playerFacingEndpointFailure(code: reported.code)
        }
        // The connection's idle timeout means the call ran out of time, not
        // that the device is offline (issue 1825).
        if let urlError = error as? URLError, urlError.code == .timedOut {
            return PlayerFacingFailure(.timedOut, .timedOut, "the response took too long")
        }
        guard let endpoint = error as? LimerickEndpointError else {
            return PlayerFacingFailure(.transport, .offline, "the response service could not be reached")
        }

        switch endpoint {
        case .invalidURL, .insecureURL, .loopbackHTTPNotAllowed:
            return PlayerFacingFailure(.protocolViolation, .unavailable, "the response service is misconfigured")
        case .invalidCredential:
            return PlayerFacingFailure.authorizationFailure
        case let .responseStatus(status):
            return PlayerFacingFailure.playerFacingHTTPFailure(status: status)
        case let .responseFailure(status, _, code, _):
            return code.map { PlayerFacingFailure.playerFacingEndpointFailure(code: $0) }
                ?? PlayerFacingFailure.playerFacingHTTPFailure(status: status)
        case .missingTerminal, .truncatedEvent:
            return PlayerFacingFailure(.missingTerminal, .offline, "the response ended before it was complete")
        case .requestTooLarge:
            return PlayerFacingFailure(.protocolViolation, .garbled, "the request was too large")
        case .responseBodyTooLarge, .lineTooLarge, .eventTooLarge, .streamTooLarge:
            return PlayerFacingFailure(.protocolViolation, .garbled, "the response was too large")
        case .unsupportedVersion, .endpointVersionMismatch:
            return PlayerFacingFailure(.protocolViolation, .unavailable, "incompatible Endpoint version")
        case .responseNotSSE, .responseNotJSON, .malformedResponse, .malformedEvent,
             .invalidUTF8, .missingRequestID, .crossCorrelation, .missingSequence,
             .outOfOrderSequence, .duplicateEvent, .duplicateTerminal, .terminalBeforeStream:
            return PlayerFacingFailure(.protocolViolation, .garbled, "the response had an unexpected format")
        }
    }
}

/// The classifications `playerFacingFailure` builds from.
private extension PlayerFacingFailure {
    static func playerFacingHTTPFailure(status: Int) -> PlayerFacingFailure {
        switch status {
        case 401, 403:
            return authorizationFailure
        case 429:
            return PlayerFacingFailure(.transport, .busy, "HTTP 429")
        case 500...599:
            return PlayerFacingFailure(.transport, .unavailable, "HTTP \(status)")
        default:
            return PlayerFacingFailure(.protocolViolation, .garbled, "HTTP \(status)")
        }
    }

    static func playerFacingEndpointFailure(code: String?) -> PlayerFacingFailure {
        let diagnostic = code ?? "unknown Endpoint error"
        switch code {
        case "AUTHENTICATION_FAILED", "AUTHORIZATION_FAILED":
            return authorizationFailure
        case "RATE_LIMITED", "QUOTA_EXCEEDED", "PROVIDER_RATE_LIMITED":
            return PlayerFacingFailure(.transport, .busy, diagnostic)
        case "PROVIDER_UNAVAILABLE", "INTERNAL_ERROR",
             "ENDPOINT_NOT_FOUND", "VERSION_NOT_FOUND", "ENDPOINT_DISABLED":
            return PlayerFacingFailure(.transport, .unavailable, diagnostic)
        case "REQUEST_TIMEOUT":
            return PlayerFacingFailure(.timedOut, .timedOut, diagnostic)
        case "REQUEST_CANCELLED":
            return PlayerFacingFailure(.interrupted, .cancelled, diagnostic)
        default:
            // OUTPUT_VALIDATION_FAILED, MODEL_ERROR, INVALID_INPUT, and any
            // code this build does not know: the reply could not be used.
            return PlayerFacingFailure(.protocolViolation, .garbled, diagnostic)
        }
    }

    static var authorizationFailure: PlayerFacingFailure {
        PlayerFacingFailure(.protocolViolation, .refused, "the device was not authorized")
    }
}

public struct EndpointReportedFailure: Error, Sendable {
    private struct Payload: Decodable {
        let code: String?
    }

    public let code: String?

    public init(payload: Data?) {
        guard let payload,
              let decoded = try? JSONDecoder().decode(Payload.self, from: payload) else {
            code = nil
            return
        }
        code = decoded.code
    }
}

private struct Phase2ProjectionStore: Sendable {
    let url: URL
    let engineURL: URL
    let modDirectory: URL?
    private(set) var restoreError: String?

    init(configuration: LaunchConfiguration, modDirectory: URL?) {
        self.modDirectory = modDirectory
        let base = configuration.draftFileURL
            ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Rundale/phase2-projection.json")
        url = base
        engineURL = base.deletingLastPathComponent().appendingPathComponent("phase2.sqlite")
        if FileManager.default.fileExists(atPath: base.path) {
            do {
                _ = try FixtureJSON.decode(Phase2Projection.self, from: Data(contentsOf: base))
                restoreError = nil
            } catch {
                // Keep the engine save authoritative and report the broken
                // presentation projection instead of silently replacing a
                // player's draft with an empty one.
                restoreError = "The saved mobile session could not be restored. Existing projection data was preserved; retrying will not overwrite it."
            }
        } else {
            restoreError = nil
        }
    }

    var engineOpenPayload: Data {
        // The world is the canonical mod, bundled with the app as game data.
        let object: [String: Any] = ["save_path": engineURL.path, "mod_dir": modDirectory?.path ?? ""]
        return (try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])) ?? Data("{}".utf8)
    }

    func restore() -> Phase2Projection? {
        guard let data = try? Data(contentsOf: url) else { return nil }
        return try? FixtureJSON.decode(Phase2Projection.self, from: data)
    }

    func save(_ projection: Phase2Projection) throws {
        if restoreError != nil,
           FileManager.default.fileExists(atPath: url.path) {
            throw Phase2ProjectionStoreError.restoreRequired(restoreError!)
        }
        let directory = url.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let data = try FixtureJSON.encode(projection)
        try data.write(to: url, options: [.atomic])
    }

    func remove() {
        let fileManager = FileManager.default
        try? fileManager.removeItem(at: url)
        try? fileManager.removeItem(at: engineURL)
        try? fileManager.removeItem(atPath: engineURL.path + "-wal")
        try? fileManager.removeItem(atPath: engineURL.path + "-shm")
    }
}

private enum Phase2ProjectionStoreError: LocalizedError {
    case restoreRequired(String)

    var errorDescription: String? {
        switch self {
        case let .restoreRequired(message):
            return "\(message) Existing projection data was preserved; retrying will not overwrite it."
        }
    }
}
