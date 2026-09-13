import Foundation
import ParishMobileFFI
import RundaleKit

/// Errors reported by the owned Parish mobile boundary.
public enum ParishRuntimeError: Error, LocalizedError, Sendable, Equatable {
    case invalidArgument(String)
    case invalidUTF8
    case invalidHandle
    case tooLarge(String)
    case protocolError(String)
    case closed
    case internalError(String)
    case operationFailed(status: Int32, message: String)
    case eventBufferOverflow

    public var errorDescription: String? {
        switch self {
        case let .invalidArgument(message): return message
        case .invalidUTF8: return "The Parish runtime returned invalid UTF-8."
        case .invalidHandle: return "The Parish runtime session is no longer valid."
        case let .tooLarge(message): return message
        case let .protocolError(message): return message
        case .closed: return "The Parish runtime session is closed."
        case let .internalError(message): return message
        case let .operationFailed(_, message): return message
        case .eventBufferOverflow: return "The Parish event stream fell behind; the session was refreshed."
        }
    }
}

/// Failure classes that may be reported by the platform Endpoint adapter.
/// Cancellation uses `stop()` and therefore remains a separate terminal
/// outcome in the Rust request state machine.
public enum ParishRuntimeFailureKind: String, Sendable {
    case transport
    case protocolViolation = "protocol"
    case missingTerminal = "missing_terminal"
    case interrupted
}

/// The actor-isolated Swift owner for one embedded Parish session.
///
/// Rust remains authoritative for request identity, validation, event order,
/// and committed state. This actor serializes calls into the opaque C handle;
/// it never retains Rust pointers or executes FFI work on `MainActor`.
public actor ParishRuntime: SessionAdapter {
    /// A consumer that stops draining events must not retain an unbounded
    /// amount of presentation data. Overflow is explicit so the application
    /// controller can refresh the authoritative snapshot and resubscribe.
    public static let eventBufferCapacity = 256

    private struct MobileOperationResult: Decodable {
        let accepted: Bool
        let logicalRequestID: LogicalRequestID?
        let attemptID: ExecutionAttemptID?
        let events: [SemanticEvent]
        let terminalOutcome: ResponseTerminalOutcome?
        let ignored: Bool
        let error: String?
        let eventCursor: EventCursor
    }

    private var handle: parish_mobile_handle_t?
    private let openingResponse: Data
    private var currentCursor = EventCursor(0)
    private var subscribers: [UUID: AsyncThrowingStream<SemanticEvent, Error>.Continuation] = [:]

    private init(handle: parish_mobile_handle_t, openingResponse: Data) {
        self.handle = handle
        self.openingResponse = openingResponse
    }

    deinit {
        // `close()` is the normal lifecycle path. The fallback matters when
        // an owner is released during cancellation or scene teardown: the
        // Rust registry must not retain a session merely because Swift lost
        // its last actor reference.
        if let handle {
            _ = parish_mobile_close(handle)
        }
    }

    /// Opens a new local session. The call should be started from a task that
    /// is not `MainActor` if opening can touch storage or content files.
    public static func openNew(options: Data = Data("{}".utf8)) throws -> ParishRuntime {
        try open(kind: PARISH_MOBILE_OPEN_NEW, request: options)
    }

    /// Resumes an existing local session from the engine-owned resume payload.
    public static func openResume(payload: Data) throws -> ParishRuntime {
        try open(kind: PARISH_MOBILE_OPEN_RESUME, request: payload)
    }

    private static func open(
        kind: parish_mobile_open_kind_t,
        request: Data
    ) throws -> ParishRuntime {
        var rawHandle: parish_mobile_handle_t = 0
        var owned = parish_mobile_owned_bytes_t(ptr: nil, len: 0)
        let status = withBorrowedBytes(request) { bytes in
            parish_mobile_open(kind, bytes, &rawHandle, &owned)
        }
        let response = try copyAndFree(owned)
        try throwIfNeeded(status, response: response)
        guard rawHandle != 0 else {
            throw ParishRuntimeError.protocolError("Parish open returned a zero handle.")
        }
        return ParishRuntime(handle: rawHandle, openingResponse: response)
    }

    /// The open response is retained as opaque JSON so the app can decode the
    /// versioned mobile snapshot without making the bridge own engine structs.
    public func openingResponseData() throws -> Data {
        try ensureOpen()
        return openingResponse
    }

    /// Closes and drains the Rust session. Calling `close` twice is harmless
    /// at the Swift layer and does not reuse the disposed token.
    public func close() throws {
        guard let handle else { return }
        let status = parish_mobile_close(handle)
        self.handle = nil
        finishSubscribers()
        try throwIfNeeded(status, response: Data())
    }

    public func events(after cursor: EventCursor?) async -> AsyncThrowingStream<SemanticEvent, Error> {
        let subscriptionID = UUID()
        let (stream, continuation) = AsyncThrowingStream<SemanticEvent, Error>.makeStream(
            bufferingPolicy: .bufferingOldest(Self.eventBufferCapacity)
        )
        continuation.onTermination = { @Sendable [weak self] _ in
            guard let self else { return }
            Task { await self.removeSubscriber(subscriptionID) }
        }
        subscribers[subscriptionID] = continuation

        do {
            let page = try readEvents(after: cursor, limit: 100)
            for event in page.events {
                guard deliver(event, to: subscriptionID, continuation: continuation) else { break }
            }
        } catch {
            subscribers.removeValue(forKey: subscriptionID)
            continuation.finish(throwing: error)
        }
        return stream
    }

    public func submit(
        text: String,
        draftID: DraftID? = nil,
        logicalRequestID: LogicalRequestID? = nil
    ) async throws -> SubmissionReceipt {
        var operation: [String: Any] = ["op": "submit", "text": text]
        if let draftID { operation["draft_id"] = draftID.rawValue }
        if let logicalRequestID { operation["logical_request_id"] = logicalRequestID.rawValue }
        let response = try dispatch(operation)
        let result = try decodeValue(MobileOperationResult.self, from: response)
        publish(result.events)
        return try submissionReceipt(from: result, isRetry: false)
    }

    public func retry(logicalRequestID: LogicalRequestID) async throws -> SubmissionReceipt {
        let response = try dispatch([
            "op": "retry",
            "logical_request_id": logicalRequestID.rawValue
        ])
        let result = try decodeValue(MobileOperationResult.self, from: response)
        publish(result.events)
        return try submissionReceipt(from: result, isRetry: true)
    }

    public func answerClarification(
        logicalRequestID: LogicalRequestID,
        choiceID: String
    ) async throws -> SubmissionReceipt {
        let response = try dispatch([
            "op": "answer_clarification",
            "logical_request_id": logicalRequestID.rawValue,
            "choice_id": choiceID
        ])
        let result = try decodeValue(MobileOperationResult.self, from: response)
        publish(result.events)
        return try submissionReceipt(from: result, isRetry: false)
    }

    public func stop() async throws -> StopReceipt {
        let response = try dispatch(["op": "stop"])
        let result = try decodeValue(MobileOperationResult.self, from: response)
        publish(result.events)
        if result.logicalRequestID == nil {
            return StopReceipt(result: .noActiveRequest)
        }
        if result.terminalOutcome == .succeeded {
            return StopReceipt(
                result: .alreadyCommitted,
                logicalRequestID: result.logicalRequestID,
                attemptID: result.attemptID
            )
        }
        return StopReceipt(
            result: .cancelled,
            logicalRequestID: result.logicalRequestID,
            attemptID: result.attemptID,
            eventID: result.events.first(where: { $0.kind == .responseCompleted })?.eventID
        )
    }

    /// Terminates the current attempt as a failed, uncommitted request. Rust
    /// owns the terminal transition; Swift supplies only the transport error
    /// message for player-facing diagnostics.
    public func fail(attemptID: ExecutionAttemptID, message: String) throws -> Data {
        let response = try dispatch([
            "op": "fail",
            "attempt_id": attemptID.rawValue,
            "message": message
        ])
        if let result = try? decodeValue(MobileOperationResult.self, from: response) {
            publish(result.events)
        }
        return response
    }

    /// Sends one raw, versioned mobile operation and returns its value JSON.
    /// This is reserved for bridge-owned operations such as `snapshot` and
    /// `pending_endpoint`; gameplay commands use the typed methods above.
    public func dispatchJSON(_ operation: Data) throws -> Data {
        let response = try dispatch(operation)
        if isEventProducingOperation(operation),
           let result = try? decodeValue(MobileOperationResult.self, from: response) {
            publish(result.events)
        }
        return response
    }

    public func snapshotJSON() throws -> Data {
        try dispatchJSON(Data(#"{"op":"snapshot"}"#.utf8))
    }

    public func pendingEndpointJSON() throws -> Data {
        try dispatchJSON(Data(#"{"op":"pending_endpoint"}"#.utf8))
    }

    /// Reads the durable semantic history through the engine's bounded page
    /// contract. Unlike the live event subscription, this remains available
    /// for events that have fallen out of the in-memory snapshot tail.
    public func readEventPage(
        after cursor: EventCursor? = nil,
        limit: Int = 100
    ) throws -> ParishEventPage {
        var operation: [String: Any] = [
            "op": "read_event_page",
            "limit": max(1, min(limit, 100))
        ]
        if let cursor { operation["after"] = cursor.rawValue }
        return try decodeValue(ParishEventPage.self, from: dispatch(operation))
    }

    /// Records a bounded transport/authentication/protocol failure against the
    /// current attempt. The engine turns it into a durable failed terminal
    /// event; it is never represented as an invalid candidate response.
    public func receiveFailure(
        attemptID: ExecutionAttemptID,
        baseRevision: StateRevision,
        kind: ParishRuntimeFailureKind,
        message: String
    ) throws -> Data {
        let operation: [String: Any] = [
            "op": "receive_failure",
            "attemptID": attemptID.rawValue,
            "baseRevision": ["rawValue": baseRevision.rawValue],
            "errorKind": kind.rawValue,
            "message": message
        ]
        let data = try JSONSerialization.data(withJSONObject: operation, options: [.sortedKeys])
        return try dispatchJSON(data)
    }

    private func dispatch(_ operation: [String: Any]) throws -> Data {
        let data = try JSONSerialization.data(withJSONObject: operation, options: [.sortedKeys])
        return try dispatch(data)
    }

    private func dispatch(_ operation: Data) throws -> Data {
        try ensureOpen()
        guard let handle else { throw ParishRuntimeError.closed }
        var owned = parish_mobile_owned_bytes_t(ptr: nil, len: 0)
        let status = withBorrowedBytes(operation) { bytes in
            parish_mobile_dispatch(handle, bytes, &owned)
        }
        let response = try copyAndFree(owned)
        try throwIfNeeded(status, response: response)
        return try decodeRawResponse(response)
    }

    private func readEvents(after cursor: EventCursor?, limit: Int) throws -> (events: [SemanticEvent], cursor: EventCursor?) {
        var operation: [String: Any] = ["op": "read_events", "limit": max(1, min(limit, 100))]
        if let cursor { operation["after"] = cursor.rawValue }
        let response = try dispatch(operation)
        let page = try decodeValue(ParishEventPage.self, from: response)
        if let pageCursor = page.nextCursor {
            currentCursor = max(currentCursor, pageCursor)
        }
        return (page.events, page.nextCursor)
    }

    private func decodeRawResponse(_ data: Data) throws -> Data {
        let object = try JSONSerialization.jsonObject(with: data)
        guard let dictionary = object as? [String: Any] else {
            return data
        }
        if let ok = dictionary["ok"] as? Bool, !ok {
            let error = dictionary["error"] as? [String: Any]
            let message = error?["message"] as? String ?? "Parish operation failed."
            throw ParishRuntimeError.operationFailed(status: -1, message: message)
        }
        let valueObject = dictionary["value"] ?? object
        return try JSONSerialization.data(withJSONObject: valueObject, options: [.sortedKeys, .fragmentsAllowed])
    }

    private func submissionReceipt(
        from result: MobileOperationResult,
        isRetry: Bool
    ) throws -> SubmissionReceipt {
        guard let logicalRequestID = result.logicalRequestID,
              let attemptID = result.attemptID else {
            throw ParishRuntimeError.protocolError(
                result.error ?? "Parish did not return request identity for an accepted operation."
            )
        }
        return SubmissionReceipt(
            logicalRequestID: logicalRequestID,
            attemptID: attemptID,
            commandEventID: result.events.first(where: { $0.kind == .playerCommand })?.eventID,
            accepted: result.accepted,
            isRetry: isRetry,
            cursor: result.eventCursor
        )
    }

    private func isEventProducingOperation(_ operation: Data) -> Bool {
        guard let object = try? JSONSerialization.jsonObject(with: operation) as? [String: Any],
              let name = object["op"] as? String else {
            return false
        }
        return [
            "submit",
            "answer_clarification",
            "retry",
            "stop",
            "fail",
            "receive_failure",
            "receive_frame",
            "receive_candidate"
        ].contains(name)
    }

    private func decodeValue<T: Decodable>(_ type: T.Type, from data: Data) throws -> T {
        try FixtureJSON.decode(type, from: data)
    }

    private func publish(_ events: [SemanticEvent]) {
        for event in events.sorted(by: { $0.sequence < $1.sequence }) {
            currentCursor = max(currentCursor, EventCursor(event.sequence.rawValue))
            for (subscriptionID, continuation) in Array(subscribers) {
                _ = deliver(event, to: subscriptionID, continuation: continuation)
            }
        }
    }

    @discardableResult
    private func deliver(
        _ event: SemanticEvent,
        to subscriptionID: UUID,
        continuation: AsyncThrowingStream<SemanticEvent, Error>.Continuation
    ) -> Bool {
        currentCursor = max(currentCursor, EventCursor(event.sequence.rawValue))
        switch continuation.yield(event) {
        case .enqueued:
            return true
        case .dropped:
            subscribers.removeValue(forKey: subscriptionID)
            continuation.finish(throwing: ParishRuntimeError.eventBufferOverflow)
            return false
        case .terminated:
            subscribers.removeValue(forKey: subscriptionID)
            return false
        @unknown default:
            subscribers.removeValue(forKey: subscriptionID)
            continuation.finish(throwing: ParishRuntimeError.eventBufferOverflow)
            return false
        }
    }

    private func removeSubscriber(_ id: UUID) {
        subscribers.removeValue(forKey: id)
    }

    private func finishSubscribers() {
        let current = subscribers
        subscribers.removeAll()
        for continuation in current.values { continuation.finish() }
    }

    private func ensureOpen() throws {
        guard handle != nil else { throw ParishRuntimeError.closed }
    }

    private static func statusCode(_ status: parish_mobile_status_t) -> Int32 {
        Int32(status.rawValue)
    }

    private static func statusError(_ status: parish_mobile_status_t, response: Data) -> ParishRuntimeError {
        switch status {
        case PARISH_MOBILE_INVALID_ARGUMENT:
            return .invalidArgument("The Parish mobile operation was invalid.")
        case PARISH_MOBILE_INVALID_UTF8:
            return .invalidUTF8
        case PARISH_MOBILE_INVALID_HANDLE:
            return .invalidHandle
        case PARISH_MOBILE_TOO_LARGE:
            return .tooLarge("The Parish mobile payload exceeded its bound.")
        case PARISH_MOBILE_PROTOCOL_ERROR:
            return .protocolError(String(data: response, encoding: .utf8) ?? "Invalid Parish response.")
        case PARISH_MOBILE_CLOSED:
            return .closed
        case PARISH_MOBILE_INTERNAL_ERROR:
            return .internalError(String(data: response, encoding: .utf8) ?? "Parish internal error.")
        case PARISH_MOBILE_OK:
            return .operationFailed(status: statusCode(status), message: "Unexpected successful status.")
        default:
            return .operationFailed(status: statusCode(status), message: "Unknown Parish status.")
        }
    }

    private static func throwIfNeeded(_ status: parish_mobile_status_t, response: Data) throws {
        guard status == PARISH_MOBILE_OK else {
            throw statusError(status, response: response)
        }
    }

    private func throwIfNeeded(_ status: parish_mobile_status_t, response: Data) throws {
        try Self.throwIfNeeded(status, response: response)
    }
}

public struct ParishEventPage: Decodable, Sendable {
    public let events: [SemanticEvent]
    public let nextCursor: EventCursor?
    public let hasMore: Bool
    public let hasOlderEvents: Bool

    private enum CodingKeys: String, CodingKey {
        case events
        case cursor
        case nextCursor
        case nextCursorSnake = "next_cursor"
        case hasMore
        case hasMoreSnake = "has_more"
        case hasOlderEvents
        case hasOlderEventsSnake = "has_older_events"
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        events = try container.decodeIfPresent([SemanticEvent].self, forKey: .events) ?? []
        nextCursor = try container.decodeIfPresent(EventCursor.self, forKey: .cursor)
            ?? container.decodeIfPresent(EventCursor.self, forKey: .nextCursor)
            ?? container.decodeIfPresent(EventCursor.self, forKey: .nextCursorSnake)
        hasMore = try container.decodeIfPresent(Bool.self, forKey: .hasMore)
            ?? container.decodeIfPresent(Bool.self, forKey: .hasMoreSnake)
            ?? false
        hasOlderEvents = try container.decodeIfPresent(Bool.self, forKey: .hasOlderEvents)
            ?? container.decodeIfPresent(Bool.self, forKey: .hasOlderEventsSnake)
            ?? false
    }
}

private func withBorrowedBytes<T>(_ data: Data, _ body: (parish_mobile_bytes_t) throws -> T) rethrows -> T {
    try data.withUnsafeBytes { rawBuffer in
        let pointer = rawBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self)
        return try body(parish_mobile_bytes_t(ptr: pointer, len: data.count))
    }
}

private func copyAndFree(_ owned: parish_mobile_owned_bytes_t) throws -> Data {
    defer { _ = parish_mobile_owned_bytes_free(owned) }
    guard let pointer = owned.ptr else { return Data() }
    return Data(bytes: pointer, count: owned.len)
}
