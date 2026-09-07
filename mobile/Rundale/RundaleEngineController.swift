import Combine
import Foundation
import ParishEndpointKit
import RundaleBridge
import RundaleKit

/// The Phase 2 application adapter. Rust owns the game/session state and its
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
    private var didStart = false
    private var didHydrateRuntime = false

    var statePublisher: AnyPublisher<SessionState, Never> { $state.eraseToAnyPublisher() }

    var currentHeader: PresentedHeader {
        PresentedHeader(
            location: state.scene?.name ?? "The crossroads",
            timeOfDay: "Late evening",
            weather: "Rain easing"
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
                transport: Phase2MockEndpointTransport()
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
        endpointTask?.cancel()
        endpointTask = nil
        guard let runtime else { return StopReceipt(result: .noActiveRequest) }
        let receipt = try await runtime.stop()
        try? refreshFromSnapshot(try await runtime.snapshotJSON())
        _ = persistSessionState()
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
        _ = choiceID
        throw FixtureAdapterError.noClarificationPending
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
                let request = try EndpointRequest(
                    url: configuration.endpointStreamURL,
                    requestID: invocation.logicalRequestID.rawValue,
                    attemptID: invocation.attemptID.rawValue,
                    idempotencyKey: invocation.idempotencyKey,
                    endpointVersion: configuration.endpointVersion,
                    body: body
                )
                for try await frame in endpointClient.stream(request) {
                    try Task.checkCancellation()
                    switch frame.kind {
                    case .progress:
                        continue
                    case .textDelta:
                        guard let text = frame.text else { continue }
                        let operation = try EndpointOperation.frame(
                            attemptID: invocation.attemptID,
                            baseRevision: invocation.baseRevision,
                            sequence: frame.sequence,
                            text: text,
                            update: .append,
                            done: false
                        )
                        let response = try await runtime.dispatchJSON(operation)
                        self?.consumeOperation(response)
                    case .final:
                        guard let payload = frame.payload else {
                            throw ParishEndpointError.malformedEvent("final output is missing")
                        }
                        let output = try FixtureJSON.decode(EndpointOutput.self, from: payload)
                        let operation = try EndpointOperation.candidate(
                            attemptID: invocation.attemptID,
                            baseRevision: invocation.baseRevision,
                            dialogue: output.dialogue,
                            metadata: [:],
                            structured: true
                        )
                        let response = try await runtime.dispatchJSON(operation)
                        self?.consumeOperation(response)
                    case .error:
                        throw ParishEndpointError.malformedEvent("Endpoint returned an error frame")
                    }
                }
            } catch is CancellationError {
                return
            } catch {
                guard !Task.isCancelled else { return }
                do {
                    let kind: ParishRuntimeFailureKind
                    if error is CancellationError {
                        kind = .interrupted
                    } else if error is ParishEndpointError,
                              (error as? ParishEndpointError) == .missingTerminal
                                || (error as? ParishEndpointError) == .truncatedEvent {
                        kind = .missingTerminal
                    } else if error is ParishEndpointError {
                        kind = .protocolViolation
                    } else {
                        kind = .transport
                    }
                    let response = try await runtime.receiveFailure(
                        attemptID: invocation.attemptID,
                        baseRevision: invocation.baseRevision,
                        kind: kind,
                        message: Self.playerFacingFailure(kind)
                    )
                    self?.consumeOperation(response)
                } catch {
                    self?.persistenceError = error.localizedDescription
                }
            }
        }
    }

    private static func playerFacingFailure(_ kind: ParishRuntimeFailureKind) -> String {
        switch kind {
        case .transport:
            return "Peig's response could not be reached. You can retry this request."
        case .missingTerminal:
            return "Peig's response ended before it was complete. You can retry this request."
        case .protocolViolation:
            return "Peig's response could not be validated. You can retry this request."
        case .interrupted:
            return "The response was interrupted. You can retry this request."
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
                    continuation.yield(.response(statusCode: 200, headers: ["content-type": "text/event-stream"]))
                    let input = identities.input.lowercased()
                    if input.contains("fail") {
                        try await Self.pause(nanoseconds: 75_000_000)
                        continuation.yield(.bytes(try Self.frame(
                            type: "error", sequence: 1, identities: identities,
                            error: ["message": "The Endpoint test response failed."]
                        )))
                    } else {
                        try await Self.pause(nanoseconds: input.contains("slow") ? 500_000_000 : 100_000_000)
                        continuation.yield(.bytes(try Self.frame(
                            type: "progress", sequence: 1, identities: identities,
                            text: "Peig listens."
                        )))
                        try await Self.pause(nanoseconds: input.contains("slow") ? 500_000_000 : 100_000_000)
                        let dialogue = "The rain keeps the old road quiet. The old church stands beyond the alder trees."
                        let chunks = [
                            "The rain keeps ",
                            "the old road quiet, ",
                            "but I remember the church beyond it."
                        ]
                        continuation.yield(.bytes(try Self.frame(
                            type: "text_delta", sequence: 2, identities: identities,
                            text: chunks[0]
                        )))
                        // Hold observable provisional stages for XCTest's
                        // accessibility polling; the production transport has
                        // no artificial pacing.
                        try await Self.pause(nanoseconds: input.contains("slow") ? 3_000_000_000 : 2_000_000_000)
                        continuation.yield(.bytes(try Self.frame(
                            type: "text_delta", sequence: 3, identities: identities,
                            text: chunks[1]
                        )))
                        try await Self.pause(nanoseconds: input.contains("slow") ? 3_000_000_000 : 2_000_000_000)
                        continuation.yield(.bytes(try Self.frame(
                            type: "text_delta", sequence: 4, identities: identities,
                            text: chunks[2]
                        )))
                        try await Self.pause(nanoseconds: 3_000_000_000)
                        continuation.yield(.bytes(try Self.frame(
                            type: "final", sequence: 5, identities: identities,
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
        let endpointVersion: Int
    }

    private static func identities(from body: Data?, url: URL) throws -> Identities {
        guard let body,
              let root = try JSONSerialization.jsonObject(with: body) as? [String: Any],
              let input = root["input"] as? [String: Any],
              let requestID = input["logicalRequestID"] as? String,
              let attemptID = input["attemptID"] as? String,
              let invocationID = input["idempotencyKey"] as? String,
              let playerInput = input["playerInput"] as? String else {
            throw ParishEndpointError.malformedEvent("mock request input is invalid")
        }
        return Identities(
            requestID: requestID,
            attemptID: attemptID,
            invocationID: invocationID,
            input: playerInput,
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
