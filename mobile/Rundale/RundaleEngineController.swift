import Combine
import Foundation
import ParishEndpointKit
import RundaleBridge
import RundaleKit

/// The native application adapter. Rust owns the game/session state and its
/// SQLite transaction boundaries; this main-actor object only projects Rust
/// semantic events for SwiftUI and owns the platform Endpoint stream.
@MainActor
final class RundaleEngineController: ObservableObject, RundaleSessionControlling {
    @Published private(set) var state: SessionState
    @Published private(set) var lastEvent: SemanticEvent?
    @Published private(set) var persistenceError: String?

    private let configuration: LaunchConfiguration
    private let projectionStore: Phase2ProjectionStore
    private let endpointClient: ParishEndpointClient
    private var completionRegistry: FixtureCompletionRegistry
    private var presentation: PresentationSession
    private var runtime: ParishRuntime?
    private var bootstrapTask: Task<Void, Never>?
    private var eventTask: Task<Void, Never>?
    private var endpointTask: Task<Void, Never>?
    private var activeEndpointRequest: EndpointRequest?
    private var didStart = false
    private var didHydrateRuntime = false
    private var engineTimeOfDay = "Morning"
    private var engineWeather = "Clear"

    var statePublisher: AnyPublisher<SessionState, Never> { $state.eraseToAnyPublisher() }

    var currentHeader: PresentedHeader {
        PresentedHeader(
            location: state.scene?.name ?? "Kilteevan Village",
            timeOfDay: engineTimeOfDay,
            weather: engineWeather
        )
    }

    var initialFollowsNewest: Bool { state.viewport.isFollowingNewest }
    var initialUnreadCount: Int { state.viewport.unreadCount }

    init(configuration: LaunchConfiguration) {
        self.configuration = configuration
        projectionStore = Phase2ProjectionStore(configuration: configuration)

        if configuration.resetFixture {
            projectionStore.remove()
        }

        let projection = projectionStore.restore()
        let initialState = SessionState(
            draft: projection?.draft ?? Draft(text: configuration.initialDraft ?? ""),
            viewport: projection?.viewport ?? TranscriptViewport()
        )
        state = initialState
        presentation = PresentationSession(state: initialState)
        // The Rust read model is not available until the runtime opens. Keep
        // slash commands ready for the composer, then replace this empty NPC
        // projection with the authoritative nearby-person list during the
        // first snapshot refresh.
        completionRegistry = FixtureCompletionRegistry(nearbyNPCs: [])

        if configuration.phase2MockTransport {
            let credentials = StaticEndpointCredentialProvider(
                ParishEndpointKit.EndpointCredentials(
                    authorizationToken: "phase2-ui-test",
                    appCheckToken: "phase2-ui-test"
                )
            )
            endpointClient = ParishEndpointClient(
                credentials: credentials,
                transport: Phase2MockEndpointTransport(),
                policy: EndpointURLPolicy(allowLoopbackHTTP: true)
            )
        } else {
            let credentials = FirebaseEndpointCredentialAdapter(
                provider: FirebaseEndpointCredentialProvider()
            )
            endpointClient = ParishEndpointClient(credentials: credentials)
        }

        if let error = projectionStore.restoreError {
            persistenceError = error
        }
    }

    deinit {
        bootstrapTask?.cancel()
        eventTask?.cancel()
        endpointTask?.cancel()
    }

    func start() {
        guard !didStart else { return }
        didStart = true
        let openPayload = projectionStore.engineOpenPayload
        bootstrapTask = Task { [weak self] in
            do {
                let runtime = try await Task.detached(priority: nil) {
                    try ParishRuntime.openResume(payload: openPayload)
                }.value
                guard let self else {
                    try? await runtime.close()
                    return
                }
                self.runtime = runtime
                try self.refreshFromSnapshot(try await runtime.snapshotJSON())
                self.startEventSubscription(runtime: runtime)
                if let invocation = try await self.pendingInvocation(runtime: runtime) {
                    self.startEndpoint(invocation, runtime: runtime)
                }
            } catch {
                guard let self else { return }
                self.persistenceError = error.localizedDescription
            }
        }
    }

    func updateDraft(_ text: String) {
        presentation.updateDraft(text)
        state = presentation.state
    }

    func restoredDraft() -> Draft? {
        projectionStore.restore()?.draft
    }

    @discardableResult
    func persistDraft(_ text: String) -> String? {
        var draft = state.draft
        draft = Draft(id: draft.id, text: text)
        return persistProjection(draft: draft, viewport: state.viewport)
    }

    @discardableResult
    func persistSessionState() -> String? {
        persistProjection(draft: state.draft, viewport: state.viewport)
    }

    func persistLifecycleSnapshot() async {
        // Rust commits acceptance, terminal transitions, and gameplay state in
        // its SQLite store. Awaiting one snapshot call drains the actor lane
        // before the separate Swift presentation projection is written.
        if let runtime {
            do {
                _ = try await runtime.snapshotJSON()
            } catch {
                persistenceError = error.localizedDescription
            }
        }
        _ = persistSessionState()
    }

    func followNewest() {
        presentation.followNewest()
        state = presentation.state
        _ = persistSessionState()
    }

    func readHistory(anchor: TranscriptAnchor? = nil) {
        presentation.readHistory(anchor: anchor)
        state = presentation.state
        _ = persistSessionState()
    }

    func submit(_ text: String) async throws -> SubmissionReceipt {
        guard let runtime else { throw ParishRuntimeError.closed }
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

    func stop() async throws -> StopReceipt {
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

    func retryLastFailed() async throws {
        guard let runtime else { throw ParishRuntimeError.closed }
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

    func step() async -> FixtureStepResult {
        FixtureStepResult(event: nil, isFinished: true)
    }

    func answerClarification(choiceID: String) async throws {
        guard let runtime else { throw ParishRuntimeError.closed }
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

    func suggestions(for text: String) -> [CompletionItem] {
        completionRegistry.suggestions(for: text)
    }

    func insert(_ item: CompletionItem, into text: String) -> String {
        completionRegistry.applying(item, to: text)
    }

    private func startEventSubscription(runtime: ParishRuntime) {
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
                self.persistenceError = error.localizedDescription
                do {
                    // The runtime journal is authoritative. Rebuild the
                    // presentation from its latest snapshot, then subscribe
                    // from the refreshed cursor so an overflow cannot leave
                    // the UI permanently behind the engine.
                    try self.refreshFromSnapshot(try await runtime.snapshotJSON())
                    self.startEventSubscription(runtime: runtime)
                } catch {
                    self.persistenceError = error.localizedDescription
                }
            }
        }
    }

    private func consume(_ event: SemanticEvent) {
        let result = presentation.apply(event)
        if result == .applied { lastEvent = event }
        state = presentation.state
    }

    private func refreshFromSnapshot(_ data: Data) throws {
        let snapshot = try FixtureJSON.decode(EngineSnapshot.self, from: data)
        engineTimeOfDay = snapshot.readModel.timeOfDay
        engineWeather = snapshot.readModel.weather
        completionRegistry = FixtureCompletionRegistry(
            nearbyNPCs: snapshot.readModel.nearbyPeople.map {
                FixtureNPCReference(id: $0.id, displayName: $0.displayName)
            }
        )

        // The first snapshot is a materialized Rust read model plus a bounded
        // event tail. Replay the tail into an empty presentation state, then
        // overlay the authoritative request/read-model metadata. This keeps
        // stable transcript IDs while avoiding the old bug where a tail that
        // no longer contained its PlayerCommand could not reconstruct the
        // request or command history.
        if !didHydrateRuntime || state.sessionID != snapshot.sessionID {
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
            pendingClarification: current.pendingClarification,
            scene: snapshot.readModel.scene.summary,
            viewport: current.viewport,
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

    private func hydrateFromSnapshot(_ snapshot: EngineSnapshot) {
        let replay = PresentationSession(state: SessionState(
            sessionID: snapshot.sessionID,
            contractVersion: snapshot.contractVersion,
            draft: state.draft,
            viewport: state.viewport
        ))
        for event in snapshot.events.sorted(by: { $0.sequence < $1.sequence }) {
            replay.apply(event)
        }
        let replayed = replay.state
        let hydrated = SessionState(
            sessionID: snapshot.sessionID,
            contractVersion: snapshot.contractVersion,
            stateRevision: snapshot.stateRevision,
            eventCursor: snapshot.eventCursor,
            transcript: replayed.transcript,
            hasOlderTranscript: replayed.hasOlderTranscript || snapshot.hasOlderEvents,
            draft: state.draft,
            requests: snapshot.requests,
            commandHistory: commandHistory(for: snapshot.requests),
            pendingClarification: replayed.pendingClarification,
            scene: snapshot.readModel.scene.summary,
            viewport: state.viewport,
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

    private func pendingInvocation(runtime: ParishRuntime) async throws -> InvocationIdentity? {
        let data = try await runtime.pendingEndpointJSON()
        guard try JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed]) as? [String: Any] != nil else {
            return nil
        }
        var invocation = try FixtureJSON.decode(InvocationIdentity.self, from: data)
        invocation.payload = data
        return invocation
    }

    private func startEndpoint(_ invocation: InvocationIdentity, runtime: ParishRuntime) {
        endpointTask?.cancel()
        let configuration = self.configuration
        let endpointClient = self.endpointClient
        endpointTask = Task { [weak self, runtime] in
            do {
                let body = try invocation.requestBody()
                guard let endpointURL = configuration.endpointURL
                    ?? (configuration.phase2MockTransport ? URL(string: "http://127.0.0.1/mock") : nil) else {
                    throw ParishEndpointError.invalidURL
                }
                let request = try EndpointRequest(
                    url: endpointURL,
                    requestID: invocation.logicalRequestID.rawValue,
                    attemptID: invocation.attemptID.rawValue,
                    idempotencyKey: invocation.idempotencyKey,
                    endpointVersion: configuration.endpointVersion,
                    policy: configuration.phase2MockTransport ? EndpointURLPolicy(allowLoopbackHTTP: true) : EndpointURLPolicy(),
                    body: body
                )
                self?.activeEndpointRequest = request
                defer {
                    if self?.activeEndpointRequest?.attemptID == request.attemptID {
                        self?.activeEndpointRequest = nil
                    }
                }
                for try await frame in endpointClient.stream(request) {
                    try Task.checkCancellation()
                    switch frame.kind {
                    case .progress:
                        continue
                    case .textDelta:
                        guard let text = frame.text, !text.isEmpty else { continue }
                        let operation = try EndpointOperation.frame(
                            attemptID: invocation.attemptID,
                            baseRevision: invocation.baseRevision,
                            sequence: frame.sequence,
                            text: text,
                            update: .append,
                            done: false
                        )
                        let response = try await runtime.dispatchJSON(operation)
                        guard !Task.isCancelled else { return }
                        self?.consumeOperation(response)
                    case .final:
                        guard let payload = frame.payload else {
                            throw ParishEndpointError.malformedEvent("final output is missing")
                        }
                        let output = try FixtureJSON.decode(EndpointOutput.self, from: payload)
                        guard !output.dialogue.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
                            throw ParishEndpointError.malformedEvent("final dialogue is empty")
                        }
                        let operation = try EndpointOperation.candidate(
                            attemptID: invocation.attemptID,
                            baseRevision: invocation.baseRevision,
                            dialogue: output.dialogue,
                            metadata: [:],
                            structured: true
                        )
                        let response = try await runtime.dispatchJSON(operation)
                        guard !Task.isCancelled else { return }
                        self?.consumeOperation(response)
                    case .error:
                        throw EndpointReportedFailure(payload: frame.error)
                    }
                }
            } catch is CancellationError {
                return
            } catch {
                guard !Task.isCancelled else { return }
                do {
                    let failure = Self.playerFacingFailure(error)
                    let response = try await runtime.receiveFailure(
                        attemptID: invocation.attemptID,
                        baseRevision: invocation.baseRevision,
                        kind: failure.kind,
                        message: failure.message
                    )
                    self?.consumeOperation(response)
                } catch {
                    self?.persistenceError = error.localizedDescription
                }
            }
        }
    }

    private static func playerFacingFailure(_ error: Error) -> PlayerFacingFailure {
        if let reported = error as? EndpointReportedFailure {
            return playerFacingEndpointFailure(code: reported.code)
        }
        guard let endpoint = error as? ParishEndpointError else {
            return PlayerFacingFailure(
                kind: .transport,
                message: "The response service could not be reached. You can retry this request."
            )
        }

        switch endpoint {
        case .invalidURL, .insecureURL, .loopbackHTTPNotAllowed:
            return PlayerFacingFailure(
                kind: .protocolViolation,
                message: "The response service is not configured correctly for this build."
            )
        case .invalidCredential:
            return authorizationFailure
        case let .responseStatus(status):
            return playerFacingHTTPFailure(status: status)
        case let .responseFailure(status, _, code, _):
            return code.map { playerFacingEndpointFailure(code: $0) }
                ?? playerFacingHTTPFailure(status: status)
        case .missingTerminal, .truncatedEvent:
            return PlayerFacingFailure(
                kind: .missingTerminal,
                message: "The response ended before it was complete. You can retry this request."
            )
        case .requestTooLarge:
            return PlayerFacingFailure(
                kind: .protocolViolation,
                message: "This request was too large for the response service. Try a shorter message."
            )
        case .responseBodyTooLarge, .lineTooLarge, .eventTooLarge, .streamTooLarge:
            return PlayerFacingFailure(
                kind: .protocolViolation,
                message: "The response was too large for the game to process. You can retry this request."
            )
        case .unsupportedVersion, .endpointVersionMismatch:
            return PlayerFacingFailure(
                kind: .protocolViolation,
                message: "The game and response service are using incompatible versions."
            )
        case .responseNotSSE, .responseNotJSON, .malformedResponse, .malformedEvent,
             .invalidUTF8, .missingRequestID, .crossCorrelation, .missingSequence,
             .outOfOrderSequence, .duplicateEvent, .duplicateTerminal, .terminalBeforeStream:
            return PlayerFacingFailure(
                kind: .protocolViolation,
                message: "The response service returned data in an unexpected format. The reply was discarded; you can retry."
            )
        }
    }

    private static func playerFacingHTTPFailure(status: Int) -> PlayerFacingFailure {
        switch status {
        case 401, 403:
            return authorizationFailure
        case 429:
            return busyFailure
        case 500...599:
            return unavailableFailure
        default:
            return PlayerFacingFailure(
                kind: .protocolViolation,
                message: "The response service rejected this request (HTTP \(status)). You can retry."
            )
        }
    }

    private static func playerFacingEndpointFailure(code: String?) -> PlayerFacingFailure {
        switch code {
        case "AUTHENTICATION_FAILED", "AUTHORIZATION_FAILED":
            return authorizationFailure
        case "RATE_LIMITED", "QUOTA_EXCEEDED", "PROVIDER_RATE_LIMITED":
            return busyFailure
        case "PROVIDER_UNAVAILABLE":
            return unavailableFailure
        case "REQUEST_TIMEOUT":
            return PlayerFacingFailure(
                kind: .transport,
                message: "The storyteller took too long to respond. You can retry this request."
            )
        case "OUTPUT_VALIDATION_FAILED":
            return PlayerFacingFailure(
                kind: .protocolViolation,
                message: "The storyteller returned a reply in the wrong format. You can retry this request."
            )
        case "MODEL_ERROR":
            return PlayerFacingFailure(
                kind: .protocolViolation,
                message: "The storyteller could not complete its reply. You can retry this request."
            )
        case "ENDPOINT_NOT_FOUND", "VERSION_NOT_FOUND", "ENDPOINT_DISABLED":
            return PlayerFacingFailure(
                kind: .protocolViolation,
                message: "The response service is unavailable for this version of the game."
            )
        case "INVALID_INPUT", "UNSUPPORTED_MEDIA_TYPE", "REQUEST_TOO_LARGE":
            return PlayerFacingFailure(
                kind: .protocolViolation,
                message: "The response service could not process this request. Try a shorter message."
            )
        case "REQUEST_CANCELLED":
            return PlayerFacingFailure(
                kind: .interrupted,
                message: "The response was cancelled before it completed. You can retry this request."
            )
        case "INTERNAL_ERROR":
            return PlayerFacingFailure(
                kind: .protocolViolation,
                message: "The response service had an internal error. You can retry this request."
            )
        default:
            return PlayerFacingFailure(
                kind: .protocolViolation,
                message: "The response service reported an error. You can retry this request."
            )
        }
    }

    private static var authorizationFailure: PlayerFacingFailure {
        PlayerFacingFailure(
            kind: .protocolViolation,
            message: "The response service could not authorize this device. Please try again later."
        )
    }

    private static var busyFailure: PlayerFacingFailure {
        PlayerFacingFailure(
            kind: .transport,
            message: "The storyteller service is busy right now. You can retry this request shortly."
        )
    }

    private static var unavailableFailure: PlayerFacingFailure {
        PlayerFacingFailure(
            kind: .transport,
            message: "The storyteller service is temporarily unavailable. You can retry this request."
        )
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

    private func persistProjection(draft: Draft, viewport: TranscriptViewport) -> String? {
        do {
            try projectionStore.save(Phase2Projection(draft: draft, viewport: viewport))
            return nil
        } catch {
            persistenceError = error.localizedDescription
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

private struct EndpointOutput: Decodable {
    let dialogue: String
}

private struct InvocationIdentity: Codable, Sendable {
    let contractVersion: PresentationContractVersion
    let sessionID: SessionID
    let logicalRequestID: LogicalRequestID
    let attemptID: ExecutionAttemptID
    let baseRevision: StateRevision
    let idempotencyKey: String
    var payload: Data?

    func requestBody() throws -> Data {
        let input: Data
        if let payload {
            input = payload
        } else {
            input = try FixtureJSON.encode(self)
        }
        let object = try JSONSerialization.jsonObject(with: input)
        return try JSONSerialization.data(withJSONObject: ["input": object], options: [.sortedKeys])
    }
}

private enum EndpointOperation {
    static func frame(
        attemptID: ExecutionAttemptID,
        baseRevision: StateRevision,
        sequence: UInt64,
        text: String,
        update: StreamUpdate,
        done: Bool
    ) throws -> Data {
        try json([
            "op": "receive_frame",
            "attempt_id": attemptID.rawValue,
            "base_revision": baseRevision.rawValue,
            "sequence": sequence,
            "text": text,
            "stream_update": update.rawValue,
            "done": done
        ])
    }

    static func candidate(
        attemptID: ExecutionAttemptID,
        baseRevision: StateRevision,
        dialogue: String,
        metadata: [String: String],
        structured: Bool
    ) throws -> Data {
        try json([
            "op": "receive_candidate",
            "attempt_id": attemptID.rawValue,
            "base_revision": baseRevision.rawValue,
            "dialogue": dialogue,
            "metadata": metadata,
            "structured": structured
        ])
    }

    private static func json(_ object: [String: Any]) throws -> Data {
        try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
    }
}

private struct Phase2Projection: Codable, Sendable {
    let draft: Draft
    let viewport: TranscriptViewport
}

private struct PlayerFacingFailure: Sendable {
    let kind: ParishRuntimeFailureKind
    let message: String
}

private struct EndpointReportedFailure: Error, Sendable {
    private struct Payload: Decodable {
        let code: String?
    }

    let code: String?

    init(payload: Data?) {
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
    private(set) var restoreError: String?

    init(configuration: LaunchConfiguration) {
        let base = configuration.draftFileURL
            ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
                .appendingPathComponent("Rundale/phase2-projection.json")
        url = base
        engineURL = base.deletingLastPathComponent().appendingPathComponent("phase2.sqlite")
        restoreError = nil
    }

    var engineOpenPayload: Data {
        let object: [String: Any] = ["save_path": engineURL.path]
        return (try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])) ?? Data("{}".utf8)
    }

    func restore() -> Phase2Projection? {
        guard let data = try? Data(contentsOf: url) else { return nil }
        return try? FixtureJSON.decode(Phase2Projection.self, from: data)
    }

    func save(_ projection: Phase2Projection) throws {
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

@MainActor
private final class FirebaseEndpointCredentialAdapter: ParishEndpointKit.EndpointCredentialProvider, @unchecked Sendable {
    private let provider: FirebaseEndpointCredentialProvider

    init(provider: FirebaseEndpointCredentialProvider) {
        self.provider = provider
    }

    nonisolated func credentials() async throws -> ParishEndpointKit.EndpointCredentials {
        let credentials = try await provider.credentials()
        return ParishEndpointKit.EndpointCredentials(
            authorizationToken: credentials.authorizationBearer,
            appCheckToken: credentials.appCheckToken
        )
    }
}

private final class Phase2MockEndpointTransport: EndpointTransport, @unchecked Sendable {
    private final class TaskBox: @unchecked Sendable {
        var task: Task<Void, Never>?
        let lock = NSLock()

        func set(_ task: Task<Void, Never>) {
            lock.lock(); defer { lock.unlock() }
            self.task = task
        }

        func cancel() {
            lock.lock(); let task = self.task; lock.unlock()
            task?.cancel()
        }
    }

    func open(_ request: URLRequest) -> EndpointByteStream {
        let box = TaskBox()
        let requestURL = request.url
        let stream = AsyncThrowingStream<EndpointTransportEvent, Error> { continuation in
            let task = Task {
                do {
                    guard let requestURL else {
                        throw ParishEndpointError.invalidURL
                    }
                    let identities = try Self.identities(from: request.httpBody, url: requestURL)
                    let input = identities.input.lowercased()
                    continuation.yield(.response(statusCode: 200, headers: ["content-type": "text/event-stream; charset=utf-8"]))
                    if input.contains("fail") {
                        continuation.yield(.bytes(try Self.frame(
                            type: "error", sequence: 1, identities: identities,
                            error: [
                                "code": "PROVIDER_UNAVAILABLE",
                                "message": "The Endpoint test response failed."
                            ]
                        )))
                    } else {
                        let dialogue: String
                        let chunks: [String]
                        if identities.speakerName == "Róisín Connolly" {
                            dialogue = "I trade spun yarn in the village when the household work allows."
                            chunks = ["I trade spun yarn ", "in the village when ", "the household work allows."]
                        } else if identities.speakerName == "Mícheál Connolly" {
                            dialogue = "The wet ground has made moving cattle difficult this week."
                            chunks = ["The wet ground ", "has made moving cattle ", "difficult this week."]
                        } else {
                            dialogue = "The rain keeps the old road quiet. The old church stands beyond the alder trees."
                            chunks = ["The rain keeps ", "the old road quiet. ", "The old church stands beyond the alder trees."]
                        }
                        // Keep the fixture observably incremental so UI tests
                        // can assert the provisional Rust presentation before
                        // the terminal candidate arrives.
                        try await Self.pause(nanoseconds: 100_000_000)
                        for (index, chunk) in chunks.enumerated() {
                            continuation.yield(.bytes(try Self.frame(
                                type: "text_delta", sequence: index + 1, identities: identities, text: chunk
                            )))
                            if index + 1 < chunks.count {
                                try await Self.pause(nanoseconds: input.contains("slow") ? 3_000_000_000 : 2_000_000_000)
                            }
                        }
                        continuation.yield(.bytes(try Self.frame(
                            type: "final", sequence: chunks.count + 1, identities: identities,
                            output: ["dialogue": dialogue]
                        )))
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            box.set(task)
            continuation.onTermination = { _ in task.cancel() }
        }
        return EndpointByteStream(events: stream, cancel: { box.cancel() })
    }

    private struct Identities {
        let requestID: String
        let attemptID: String
        let invocationID: String
        let input: String
        let speakerName: String
        let endpointVersion: Int
    }

    private static func identities(from body: Data?, url: URL) throws -> Identities {
        guard let body,
              let root = try JSONSerialization.jsonObject(with: body) as? [String: Any],
              let input = root["input"] as? [String: Any],
              let requestID = input["logicalRequestID"] as? String,
              let attemptID = input["attemptID"] as? String,
              let invocationID = input["idempotencyKey"] as? String,
              let playerInput = input["playerInput"] as? String,
              let speaker = input["speaker"] as? [String: Any],
              let speakerName = speaker["displayName"] as? String else {
            throw ParishEndpointError.malformedEvent("mock request input is invalid")
        }
        return Identities(
            requestID: requestID,
            attemptID: attemptID,
            invocationID: invocationID,
            input: playerInput,
            speakerName: speakerName,
            endpointVersion: Self.endpointVersion(from: url)
        )
    }

    private static func endpointVersion(from url: URL) -> Int {
        let components = url.pathComponents
        guard let versionsIndex = components.firstIndex(of: "versions"),
              versionsIndex + 1 < components.count,
              let version = Int(components[versionsIndex + 1]),
              version > 0 else {
            return 1
        }
        return version
    }

    private static func frame(
        type: String,
        sequence: Int,
        identities: Identities,
        text: String? = nil,
        output: [String: Any]? = nil,
        error: [String: Any]? = nil
    ) throws -> Data {
        let eventID = "\(identities.invocationID):\(sequence)"
        var payload: [String: Any] = [
            "contract_version": 1,
            "request_id": identities.requestID,
            "attempt_id": identities.attemptID,
            "invocation_id": identities.invocationID,
            "event_id": eventID,
            "sequence": sequence,
            "type": type,
            "endpoint_version": identities.endpointVersion
        ]
        if let text { payload["text"] = text }
        if let output { payload["output"] = output }
        if let error { payload["error"] = error }
        let data = try JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
        var result = Data("event: \(type)\nid: \(eventID)\ndata: ".utf8)
        result.append(data)
        result.append(Data("\n\n".utf8))
        return result
    }

    private static func pause(nanoseconds: UInt64) async throws {
        try await Task.sleep(nanoseconds: nanoseconds)
    }
}
