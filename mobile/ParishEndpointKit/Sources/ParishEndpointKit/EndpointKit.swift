import Foundation
import CoreFoundation

#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// Errors raised by the mobile Endpoint transport or its protocol validator.
/// These errors are deliberately small and stable: the player-facing layer can
/// turn them into a useful message without exposing provider details.
public enum ParishEndpointError: Error, Equatable, Sendable, LocalizedError {
    case invalidURL
    case insecureURL
    case loopbackHTTPNotAllowed
    case invalidCredential
    case responseStatus(Int)
    case responseFailure(status: Int, requestID: String, code: String?, message: String?)
    case responseNotSSE
    case responseNotJSON
    case responseBodyTooLarge
    case malformedResponse(String)
    case malformedEvent(String)
    case invalidUTF8
    case lineTooLarge
    case eventTooLarge
    case streamTooLarge
    case requestTooLarge
    case unsupportedVersion(Int)
    case endpointVersionMismatch(expected: Int, actual: Int)
    case missingRequestID
    case crossCorrelation(expected: String, actual: String)
    case missingSequence
    case outOfOrderSequence(expected: UInt64, actual: UInt64)
    case duplicateEvent(String)
    case duplicateTerminal
    case terminalBeforeStream
    case missingTerminal
    case truncatedEvent

    public var errorDescription: String? {
        switch self {
        case .invalidURL: return "The Parish Endpoint URL is invalid."
        case .insecureURL: return "Parish Endpoint requests must use HTTPS."
        case .loopbackHTTPNotAllowed: return "HTTP is allowed only for the controlled loopback test transport."
        case .invalidCredential: return "The Parish Endpoint credentials are invalid."
        case let .responseStatus(status): return "The Parish Endpoint returned HTTP \(status)."
        case let .responseFailure(status, _, _, _): return "The Parish Endpoint returned HTTP \(status) for this request."
        case .responseNotSSE: return "The Parish Endpoint did not return a streaming response."
        case .responseNotJSON: return "The Parish Endpoint did not return a JSON response."
        case .responseBodyTooLarge: return "The Parish Endpoint response is too large."
        case let .malformedResponse(message): return "The Parish Endpoint response is malformed: \(message)"
        case let .malformedEvent(message): return "The Parish Endpoint stream is malformed: \(message)"
        case .invalidUTF8: return "The Parish Endpoint stream contains invalid UTF-8."
        case .lineTooLarge: return "The Parish Endpoint stream line is too large."
        case .eventTooLarge: return "The Parish Endpoint stream event is too large."
        case .streamTooLarge: return "The Parish Endpoint stream is too large."
        case .requestTooLarge: return "The Parish Endpoint request is too large."
        case let .unsupportedVersion(version): return "The Parish Endpoint stream version \(version) is unsupported."
        case let .endpointVersionMismatch(expected, actual): return "The Parish Endpoint returned Endpoint version \(actual); expected pinned version \(expected)."
        case .missingRequestID: return "The Parish Endpoint stream omitted its request identity."
        case let .crossCorrelation(expected, actual): return "The Parish Endpoint returned request \(actual), expected \(expected)."
        case .missingSequence: return "The Parish Endpoint stream omitted an event sequence."
        case let .outOfOrderSequence(expected, actual): return "The Parish Endpoint stream sequence \(actual) arrived; expected \(expected)."
        case let .duplicateEvent(id): return "The Parish Endpoint repeated event \(id)."
        case .duplicateTerminal: return "The Parish Endpoint returned more than one terminal event."
        case .terminalBeforeStream: return "The Parish Endpoint returned a terminal event before any response data."
        case .missingTerminal: return "The Parish Endpoint stream ended without a terminal event."
        case .truncatedEvent: return "The Parish Endpoint stream ended in the middle of an event."
        }
    }
}

/// Hard ceilings for untrusted Endpoint input and asynchronous buffering.
/// Callers may choose stricter limits, but cannot disable these bounds by
/// passing an arbitrarily large value to a parser or validator initializer.
public enum EndpointResourceLimits {
    public static let maximumRequestBodyBytes = 256 * 1024
    public static let maximumLineBytes = 16 * 1024
    public static let maximumEventBytes = 64 * 1024
    public static let maximumFrames = 4_096
    public static let maximumBufferedTransportEvents = 64 * 1024
    public static let maximumCompletedResponseBytes = 256 * 1024
    public static let maximumDialogueScalars = 8_192
}

/// Credentials are fetched for each request so the Endpoint client never
/// persists a Firebase token or an App Check token. The provider is injected
/// by the host, which keeps this package independent of Firebase SDK types.
public struct EndpointCredentials: Equatable, Sendable {
    public let authorizationToken: String
    public let appCheckToken: String?

    public init(authorizationToken: String, appCheckToken: String? = nil) {
        self.authorizationToken = authorizationToken
        self.appCheckToken = appCheckToken
    }
}

public protocol EndpointCredentialProvider: Sendable {
    func credentials() async throws -> EndpointCredentials
}

public struct StaticEndpointCredentialProvider: EndpointCredentialProvider {
    public let value: EndpointCredentials

    public init(_ value: EndpointCredentials) {
        self.value = value
    }

    public func credentials() async throws -> EndpointCredentials { value }
}

/// A request body is kept as already-encoded JSON. This prevents the mobile
/// transport from taking ownership of the game or Endpoint schema.
public struct EndpointRequest: Equatable, Sendable {
    public let url: URL
    public let requestID: String
    public let attemptID: String?
    public let invocationID: String?
    public let idempotencyKey: String
    public let endpointVersion: Int
    public let body: Data

    public init(
        url: URL,
        requestID: String,
        attemptID: String? = nil,
        invocationID: String? = nil,
        idempotencyKey: String? = nil,
        endpointVersion: Int = 1,
        policy: EndpointURLPolicy = EndpointURLPolicy(),
        body: Data
    ) throws {
        guard !requestID.isEmpty else { throw ParishEndpointError.missingRequestID }
        guard endpointVersion > 0 else { throw ParishEndpointError.invalidURL }
        guard url.host?.isEmpty == false else { throw ParishEndpointError.invalidURL }
        guard body.count <= EndpointResourceLimits.maximumRequestBodyBytes else {
            throw ParishEndpointError.requestTooLarge
        }
        try policy.validate(url)
        self.url = url
        self.requestID = requestID
        self.attemptID = attemptID
        self.invocationID = invocationID
        self.idempotencyKey = idempotencyKey ?? requestID
        self.endpointVersion = endpointVersion
        self.body = body
    }
}

/// One decoded SSE event. `data` is the exact UTF-8 JSON payload after the
/// SSE `data:` lines have been joined with newlines.
public struct SSEEvent: Equatable, Sendable {
    public let event: String?
    public let id: String?
    public let data: Data

    public init(event: String?, id: String? = nil, data: Data) {
        self.event = event
        self.id = id
        self.data = data
    }
}

/// Incremental, bounded SSE parser. It consumes arbitrary byte chunks and
/// never retains more than the configured event/line budget.
public struct BoundedSSEParser: Sendable {
    public struct Limits: Equatable, Sendable {
        public let maximumLineBytes: Int
        public let maximumEventBytes: Int

        public init(
            maximumLineBytes: Int = EndpointResourceLimits.maximumLineBytes,
            maximumEventBytes: Int = EndpointResourceLimits.maximumEventBytes
        ) {
            self.maximumLineBytes = min(
                max(1, maximumLineBytes),
                EndpointResourceLimits.maximumLineBytes
            )
            self.maximumEventBytes = min(
                max(1, maximumEventBytes),
                EndpointResourceLimits.maximumEventBytes
            )
        }
    }

    public let limits: Limits
    private var buffer = Data()
    private var eventName: String?
    private var eventID: String?
    private var dataLines: [Data] = []
    private var currentEventBytes = 0

    public init(limits: Limits = Limits()) {
        self.limits = limits
    }

    /// Appends raw response bytes and returns every complete event available.
    public mutating func append(_ bytes: Data) throws -> [SSEEvent] {
        guard !bytes.isEmpty else { return [] }
        var events: [SSEEvent] = []
        var offset = bytes.startIndex
        let chunkSize = limits.maximumLineBytes + 1
        while offset < bytes.endIndex {
            let remaining = bytes.distance(from: offset, to: bytes.endIndex)
            let length = min(chunkSize, remaining)
            let end = bytes.index(offset, offsetBy: length)
            // Never append an untrusted transport chunk wholesale. The
            // parser's temporary buffer is bounded to roughly two line
            // budgets even when a caller supplies a multi-megabyte Data.
            buffer.append(bytes[offset..<end])
            while let newline = buffer.firstIndex(of: 0x0A) {
                let line = Data(buffer[..<newline])
                buffer.removeSubrange(...newline)
                events.append(contentsOf: try consume(line))
            }
            if buffer.count > limits.maximumLineBytes {
                throw ParishEndpointError.lineTooLarge
            }
            offset = end
        }
        return events
    }

    /// A valid SSE stream must end at an event boundary. A partial line or
    /// event is treated as a protocol truncation rather than silently lost.
    public mutating func finish() throws -> [SSEEvent] {
        guard buffer.isEmpty, eventName == nil, eventID == nil, dataLines.isEmpty else {
            throw ParishEndpointError.truncatedEvent
        }
        return []
    }

    private mutating func consume(_ rawLine: Data) throws -> [SSEEvent] {
        var line = rawLine
        if line.last == 0x0D { line.removeLast() }
        guard line.count <= limits.maximumLineBytes else { throw ParishEndpointError.lineTooLarge }
        currentEventBytes += line.count + 1
        guard currentEventBytes <= limits.maximumEventBytes else { throw ParishEndpointError.eventTooLarge }

        if line.isEmpty {
            defer {
                eventName = nil
                eventID = nil
                dataLines.removeAll(keepingCapacity: true)
                currentEventBytes = 0
            }
            guard !dataLines.isEmpty || eventName != nil || eventID != nil else { return [] }
            guard !dataLines.isEmpty else {
                throw ParishEndpointError.malformedEvent("event has no data")
            }
            let payload = dataLines.enumerated().reduce(into: Data()) { result, item in
                result.append(item.element)
                if item.offset + 1 < dataLines.count { result.append(0x0A) }
            }
            return [SSEEvent(event: eventName, id: eventID, data: payload)]
        }

        if line.first == 0x3A { return [] } // SSE comment/keepalive.
        guard let colon = line.firstIndex(of: 0x3A) else {
            throw ParishEndpointError.malformedEvent("field has no colon")
        }
        let fieldData = Data(line[..<colon])
        var value = Data(line[line.index(after: colon)...])
        if value.first == 0x20 { value.removeFirst() }
        guard let field = String(data: fieldData, encoding: .utf8),
              let valueString = String(data: value, encoding: .utf8) else {
            throw ParishEndpointError.invalidUTF8
        }
        switch field {
        case "event": eventName = valueString
        case "id": eventID = valueString
        case "data": dataLines.append(value)
        default:
            throw ParishEndpointError.malformedEvent("unknown field \(field)")
        }
        return []
    }
}

/// Wire-level frame emitted by an Endpoint. `payload` is the optional JSON
/// object from `data`; `text` is a convenience for text streaming endpoints.
public struct EndpointStreamFrame: Equatable, Sendable {
    public enum Kind: String, Codable, Sendable {
        case progress
        case textDelta = "text_delta"
        case final
        case error

        public var isTerminal: Bool {
            switch self {
            case .final, .error: return true
            case .progress, .textDelta: return false
            }
        }
    }

    public let version: Int
    public let endpointVersion: Int
    public let requestID: String
    public let attemptID: String?
    public let sequence: UInt64
    public let invocationID: String?
    public let eventID: String?
    public let kind: Kind
    public let text: String?
    public let payload: Data?
    public let error: Data?
    public let terminal: Bool

    public init(
        version: Int = 1,
        endpointVersion: Int = 1,
        requestID: String,
        attemptID: String? = nil,
        sequence: UInt64,
        invocationID: String? = nil,
        eventID: String? = nil,
        kind: Kind,
        text: String? = nil,
        payload: Data? = nil,
        error: Data? = nil,
        terminal: Bool? = nil
    ) {
        self.version = version
        self.endpointVersion = endpointVersion
        self.requestID = requestID
        self.attemptID = attemptID
        self.sequence = sequence
        self.invocationID = invocationID
        self.eventID = eventID
        self.kind = kind
        self.text = text
        self.payload = payload
        self.error = error
        self.terminal = terminal ?? kind.isTerminal
    }
}

/// Validates frame identity and ordering across a complete response stream.
public struct EndpointStreamValidator: Sendable {
    public let expectedRequestID: String
    public let expectedAttemptID: String?
    public let expectedInvocationID: String?
    public let expectedEndpointVersion: Int
    public let maximumFrames: Int

    private var lastSequence: UInt64?
    private var seenEventIDs = Set<String>()
    private var frameCount = 0
    private var sawPayload = false
    private var terminalSeen = false
    private var validatedInvocationID: String?

    public init(
        expectedRequestID: String,
        expectedAttemptID: String? = nil,
        expectedInvocationID: String? = nil,
        expectedEndpointVersion: Int = 1,
        maximumFrames: Int = 512
    ) {
        self.expectedRequestID = expectedRequestID
        self.expectedAttemptID = expectedAttemptID
        self.expectedInvocationID = expectedInvocationID
        self.expectedEndpointVersion = max(1, expectedEndpointVersion)
        self.maximumFrames = min(max(1, maximumFrames), EndpointResourceLimits.maximumFrames)
    }

    public mutating func accept(_ event: SSEEvent) throws -> EndpointStreamFrame {
        let frame = try Self.decode(event)
        guard frame.version == 1 else { throw ParishEndpointError.unsupportedVersion(frame.version) }
        guard frame.endpointVersion == expectedEndpointVersion else {
            throw ParishEndpointError.endpointVersionMismatch(
                expected: expectedEndpointVersion,
                actual: frame.endpointVersion
            )
        }
        guard let invocationID = frame.invocationID, !invocationID.isEmpty else {
            throw ParishEndpointError.malformedEvent("invocation_id is missing")
        }
        if let expectedInvocationID {
            guard invocationID == expectedInvocationID else {
                throw ParishEndpointError.crossCorrelation(expected: expectedInvocationID, actual: invocationID)
            }
        }
        if let validatedInvocationID {
            guard invocationID == validatedInvocationID else {
                throw ParishEndpointError.crossCorrelation(expected: validatedInvocationID, actual: invocationID)
            }
        }
        guard let attemptID = frame.attemptID, !attemptID.isEmpty else {
            throw ParishEndpointError.malformedEvent("attempt_id is missing")
        }
        guard frame.requestID == expectedRequestID else {
            throw ParishEndpointError.crossCorrelation(expected: expectedRequestID, actual: frame.requestID)
        }
        if let expectedAttemptID {
            guard attemptID == expectedAttemptID else {
                throw ParishEndpointError.crossCorrelation(expected: expectedAttemptID, actual: attemptID)
            }
        }
        guard frameCount < maximumFrames else { throw ParishEndpointError.streamTooLarge }
        frameCount += 1

        guard let eventID = event.id ?? frame.eventID, !eventID.isEmpty else {
            throw ParishEndpointError.malformedEvent("event_id is missing")
        }
        guard eventID == "\(invocationID):\(frame.sequence)" else {
            throw ParishEndpointError.malformedEvent("event_id does not match invocation_id and sequence")
        }
        guard seenEventIDs.insert(eventID).inserted else {
            throw ParishEndpointError.duplicateEvent(eventID)
        }
        if let previous = lastSequence {
            let expected = previous &+ 1
            guard frame.sequence == expected else {
                throw ParishEndpointError.outOfOrderSequence(expected: expected, actual: frame.sequence)
            }
        } else if frame.sequence != 1 {
            throw ParishEndpointError.outOfOrderSequence(expected: 1, actual: frame.sequence)
        }
        lastSequence = frame.sequence

        if frame.kind == .progress || frame.kind == .textDelta || frame.text != nil || frame.payload != nil {
            sawPayload = true
        }
        if frame.terminal {
            // An Endpoint may fail before it has emitted a text delta. That
            // is still a valid terminal response and lets Rust persist the
            // failure/retry state instead of misreporting truncation.
            guard sawPayload || frame.kind == .error else { throw ParishEndpointError.terminalBeforeStream }
            guard !terminalSeen else { throw ParishEndpointError.duplicateTerminal }
            terminalSeen = true
        } else if terminalSeen {
            throw ParishEndpointError.malformedEvent("frame arrived after terminal")
        }
        // The Endpoint may assign its invocation identity after the request
        // is sent. When the caller has no expected value, pin the first
        // nonempty identity only after the complete frame has validated so a
        // malformed first frame cannot establish the stream's correlation.
        if validatedInvocationID == nil {
            validatedInvocationID = invocationID
        }
        return frame
    }

    public mutating func finish() throws {
        guard terminalSeen else { throw ParishEndpointError.missingTerminal }
    }

    private static func decode(_ event: SSEEvent) throws -> EndpointStreamFrame {
        guard let object = try? JSONSerialization.jsonObject(with: event.data),
              let values = object as? [String: Any] else {
            throw ParishEndpointError.malformedEvent("data is not a JSON object")
        }
        func string(_ names: [String]) -> String? {
            names.lazy.compactMap { values[$0] as? String }.first
        }
        func integer(_ names: [String]) -> UInt64? {
            for name in names {
                guard let value = values[name] else { continue }
                if let value = value as? NSNumber {
                    // JSONSerialization bridges both booleans and numbers to
                    // NSNumber. CFBoolean must not become sequence/version 1
                    // through int64Value, and floating values must be exact
                    // nonnegative integers before entering the wire model.
                    guard CFGetTypeID(value) != CFBooleanGetTypeID() else { return nil }
                    if CFNumberIsFloatType(value) {
                        var decimal = value.decimalValue
                        var rounded = Decimal()
                        NSDecimalRound(&rounded, &decimal, 0, .plain)
                        guard decimal == rounded, rounded >= 0 else { return nil }
                        return UInt64(NSDecimalNumber(decimal: rounded).stringValue)
                    }
                    return UInt64(value.stringValue)
                }
                if let value = value as? String, let parsed = UInt64(value) { return parsed }
                return nil
            }
            return nil
        }
        guard let rawContractVersion = integer(["contract_version"]), rawContractVersion <= UInt64(Int.max) else {
            throw ParishEndpointError.malformedEvent("contract_version is missing")
        }
        let version = Int(rawContractVersion)
        guard version == 1 else { throw ParishEndpointError.unsupportedVersion(version) }
        guard let endpointVersion = integer(["endpoint_version"]), endpointVersion > 0,
              endpointVersion <= UInt64(Int.max) else {
            throw ParishEndpointError.malformedEvent("endpoint_version is missing")
        }
        guard let requestID = string(["request_id", "requestId"]) else {
            throw ParishEndpointError.missingRequestID
        }
        guard let sequence = integer(["sequence"]), sequence > 0 else {
            throw ParishEndpointError.missingSequence
        }
        guard let rawKind = string(["type"]), !rawKind.isEmpty else {
            throw ParishEndpointError.malformedEvent("unknown frame kind")
        }
        guard let eventName = event.event, !eventName.isEmpty else {
            throw ParishEndpointError.malformedEvent("SSE event type is missing")
        }
        if eventName != rawKind {
            throw ParishEndpointError.malformedEvent("SSE event type does not match frame type")
        }
        let kind: EndpointStreamFrame.Kind
        switch rawKind {
        case "progress": kind = .progress
        case "text_delta": kind = .textDelta
        case "final": kind = .final
        case "error": kind = .error
        default:
            throw ParishEndpointError.malformedEvent("unknown frame kind")
        }
        let text = string(["text"])
        var payload: Data?
        if let value = values["output"] {
            payload = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys, .fragmentsAllowed])
        }
        var error: Data?
        if let value = values["error"] {
            error = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys, .fragmentsAllowed])
        }
        if kind == .final {
            guard let output = values["output"] as? [String: Any],
                  Set(output.keys) == ["dialogue"],
                  let dialogue = output["dialogue"] as? String,
                  !dialogue.isEmpty,
                  dialogue.unicodeScalars.count <= EndpointResourceLimits.maximumDialogueScalars else {
                throw ParishEndpointError.malformedEvent("final output.dialogue is missing")
            }
        }
        if kind == .error, values["error"] == nil {
            throw ParishEndpointError.malformedEvent("error payload is missing")
        }
        if let rawTerminal = values["terminal"] {
            guard let number = rawTerminal as? NSNumber,
                  CFGetTypeID(number) == CFBooleanGetTypeID(),
                  let terminal = rawTerminal as? Bool else {
                throw ParishEndpointError.malformedEvent("terminal flag must be boolean")
            }
            if terminal != kind.isTerminal {
                throw ParishEndpointError.malformedEvent("terminal flag does not match frame type")
            }
        }
        guard let eventID = string(["event_id", "eventId"]), !eventID.isEmpty else {
            throw ParishEndpointError.malformedEvent("event_id is missing")
        }
        if let sseEventID = event.id, sseEventID != eventID {
            throw ParishEndpointError.malformedEvent("SSE id does not match event_id")
        }
        return EndpointStreamFrame(
            version: version,
            endpointVersion: Int(endpointVersion),
            requestID: requestID,
            attemptID: string(["attempt_id", "attemptId"]),
            sequence: sequence,
            invocationID: string(["invocation_id", "invocationId"]),
            eventID: eventID,
            kind: kind,
            text: text,
            payload: payload,
            error: error,
            terminal: kind.isTerminal
        )
    }
}

/// A transport event includes the HTTP response before any body bytes. This
/// makes the client testable without a socket while preserving URLSession's
/// streaming behavior in production.
public enum EndpointTransportEvent: Sendable {
    case response(statusCode: Int, headers: [String: String])
    case bytes(Data)
}

public struct EndpointByteStream: Sendable {
    public let events: AsyncThrowingStream<EndpointTransportEvent, Error>
    private let cancelOperation: @Sendable () -> Void

    public init(
        events: AsyncThrowingStream<EndpointTransportEvent, Error>,
        cancel: @escaping @Sendable () -> Void = {}
    ) {
        self.events = events
        self.cancelOperation = cancel
    }

    public func cancel() { cancelOperation() }
}

/// One completed (non-streaming) Endpoint response. The request identity is
/// retained even though the Parish response body is the schema-defined output
/// object; callers can use it when correlating the result with the pending
/// mobile attempt. No retry is performed by this boundary.
public struct EndpointCompletedResponse: Equatable, Sendable {
    public let requestID: String
    public let statusCode: Int
    public let body: Data

    public init(requestID: String, statusCode: Int, body: Data) {
        self.requestID = requestID
        self.statusCode = statusCode
        self.body = body
    }

    /// Validates the completed body as one bounded JSON object. Schema
    /// validation remains with the caller because the schema is deployed
    /// independently of this transport package.
    public func validatedJSON() throws -> [String: Any] {
        guard let object = try? JSONSerialization.jsonObject(with: body),
              let object = object as? [String: Any] else {
            throw ParishEndpointError.malformedResponse("output is not a JSON object")
        }
        return object
    }

    /// Validates the deployed v1 output schema exactly. Codable decoding alone
    /// is insufficient here because it ignores unknown properties.
    public func validatedDialogue() throws -> String {
        let object = try validatedJSON()
        guard Set(object.keys) == ["dialogue"],
              let dialogue = object["dialogue"] as? String,
              !dialogue.isEmpty,
              dialogue.unicodeScalars.count <= EndpointResourceLimits.maximumDialogueScalars else {
            throw ParishEndpointError.malformedResponse("output does not match the dialogue schema")
        }
        return dialogue
    }
}

public protocol EndpointTransport: Sendable {
    func open(_ request: URLRequest) -> EndpointByteStream
}

/// Production URLSession transport. It emits response metadata first and then
/// forwards each byte chunk without buffering the body or retrying failures.
public final class URLSessionEndpointTransport: EndpointTransport, @unchecked Sendable {
    private let configuration: URLSessionConfiguration
    private let delegateQueue: OperationQueue?

    public init(session: URLSession = .shared) {
        configuration = session.configuration
        delegateQueue = session.delegateQueue
    }

    public func open(_ request: URLRequest) -> EndpointByteStream {
        let box = URLSessionEndpointTaskBox()
        let stream = AsyncThrowingStream<EndpointTransportEvent, Error>(
            bufferingPolicy: .bufferingOldest(EndpointResourceLimits.maximumBufferedTransportEvents)
        ) { continuation in
            let delegate = URLSessionEndpointDelegate(continuation: continuation, taskBox: box)
            let session = URLSession(
                configuration: configuration,
                delegate: delegate,
                delegateQueue: delegateQueue
            )
            let task = session.dataTask(with: request)
            box.start(session: session, task: task)
            continuation.onTermination = { _ in box.cancel() }
        }
        return EndpointByteStream(events: stream, cancel: { box.cancel() })
    }
}

private final class URLSessionEndpointTaskBox: @unchecked Sendable {
    private let lock = NSLock()
    private var session: URLSession?
    private var task: URLSessionDataTask?
    private var cancelled = false

    func start(session: URLSession, task: URLSessionDataTask) {
        lock.lock()
        let shouldCancel = cancelled
        if !shouldCancel {
            self.session = session
            self.task = task
        }
        lock.unlock()
        if shouldCancel {
            task.cancel()
            session.invalidateAndCancel()
        } else {
            task.resume()
        }
    }

    func cancel() {
        lock.lock()
        cancelled = true
        let task = task
        let session = session
        self.task = nil
        self.session = nil
        lock.unlock()
        task?.cancel()
        session?.invalidateAndCancel()
    }

    func finish() {
        lock.lock()
        let session = session
        task = nil
        self.session = nil
        lock.unlock()
        session?.finishTasksAndInvalidate()
    }
}

private final class URLSessionEndpointDelegate: NSObject, URLSessionDataDelegate, @unchecked Sendable {
    private let continuation: AsyncThrowingStream<EndpointTransportEvent, Error>.Continuation
    private let taskBox: URLSessionEndpointTaskBox

    init(
        continuation: AsyncThrowingStream<EndpointTransportEvent, Error>.Continuation,
        taskBox: URLSessionEndpointTaskBox
    ) {
        self.continuation = continuation
        self.taskBox = taskBox
    }

    func urlSession(
        _ session: URLSession,
        dataTask: URLSessionDataTask,
        didReceive response: URLResponse,
        completionHandler: @escaping (URLSession.ResponseDisposition) -> Void
    ) {
        let headers = (response as? HTTPURLResponse)?.allHeaderFields.reduce(into: [String: String]()) { result, entry in
            result[String(describing: entry.key).lowercased()] = String(describing: entry.value)
        } ?? [:]
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        switch continuation.yield(.response(statusCode: status, headers: headers)) {
        case .enqueued:
            completionHandler(.allow)
        case .dropped:
            continuation.finish(throwing: ParishEndpointError.streamTooLarge)
            completionHandler(.cancel)
            taskBox.cancel()
        case .terminated:
            completionHandler(.cancel)
            taskBox.cancel()
        @unknown default:
            continuation.finish(throwing: ParishEndpointError.streamTooLarge)
            completionHandler(.cancel)
            taskBox.cancel()
        }
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
        guard !data.isEmpty else { return }
        switch continuation.yield(.bytes(data)) {
        case .enqueued:
            break
        case .dropped:
            continuation.finish(throwing: ParishEndpointError.streamTooLarge)
            taskBox.cancel()
        case .terminated:
            taskBox.cancel()
        @unknown default:
            continuation.finish(throwing: ParishEndpointError.streamTooLarge)
            taskBox.cancel()
        }
    }

    func urlSession(
        _ session: URLSession,
        task: URLSessionTask,
        didCompleteWithError error: (any Error)?
    ) {
        if let error {
            if (error as NSError).code == NSURLErrorCancelled {
                continuation.finish(throwing: CancellationError())
            } else {
                continuation.finish(throwing: error)
            }
        } else {
            continuation.finish()
        }
        taskBox.finish()
    }
}

public struct EndpointURLPolicy: Sendable, Equatable {
    private let allowLoopbackHTTP: Bool

    public init(allowLoopbackHTTP: Bool = false) {
        self.allowLoopbackHTTP = allowLoopbackHTTP
    }

    public func validate(_ url: URL) throws {
        guard url.scheme?.lowercased() == "http" || url.scheme?.lowercased() == "https" else {
            throw ParishEndpointError.invalidURL
        }
        if url.scheme?.lowercased() == "https" { return }
        guard allowLoopbackHTTP, Self.isLoopback(url) else { throw ParishEndpointError.loopbackHTTPNotAllowed }
    }

    fileprivate static func isLoopback(_ url: URL) -> Bool {
        guard let host = url.host?.lowercased() else { return false }
        return host == "localhost" || host == "127.0.0.1" || host == "::1"
    }
}

/// Mobile-safe Endpoint client. The Endpoint URL, headers, transport, and
/// credentials are all injectable so deterministic protocol tests do not use
/// the network and production never embeds a provider key.
public final class ParishEndpointClient: @unchecked Sendable {
    private let credentials: any EndpointCredentialProvider
    private let transport: any EndpointTransport
    private let policy: EndpointURLPolicy
    private let limits: BoundedSSEParser.Limits
    private let maximumFrames: Int
    private let maximumCompletedResponseBytes: Int

    public init(
        credentials: any EndpointCredentialProvider,
        transport: any EndpointTransport = URLSessionEndpointTransport(),
        policy: EndpointURLPolicy = EndpointURLPolicy(),
        limits: BoundedSSEParser.Limits = .init(),
        maximumFrames: Int = 512,
        maximumCompletedResponseBytes: Int = EndpointResourceLimits.maximumCompletedResponseBytes
    ) {
        self.credentials = credentials
        self.transport = transport
        self.policy = policy
        self.limits = limits
        self.maximumFrames = min(
            max(1, maximumFrames),
            EndpointResourceLimits.maximumFrames
        )
        self.maximumCompletedResponseBytes = min(
            max(1, maximumCompletedResponseBytes),
            EndpointResourceLimits.maximumCompletedResponseBytes
        )
    }

    /// Sends the schema-defined request and waits for Parish's completed JSON
    /// response. This is deliberately separate from `stream`: the deployed
    /// Parish contract returns output directly, and this method never
    /// interprets body bytes as token deltas or retries an ambiguous timeout.
    public func complete(_ endpointRequest: EndpointRequest) async throws -> EndpointCompletedResponse {
        try Task.checkCancellation()
        try policy.validate(endpointRequest.url)
        let auth = try await credentials.credentials()
        try Task.checkCancellation()
        guard !auth.authorizationToken.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw ParishEndpointError.invalidCredential
        }

        var request = URLRequest(url: endpointRequest.url)
        request.httpMethod = "POST"
        request.httpBody = endpointRequest.body
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue("Bearer \(auth.authorizationToken)", forHTTPHeaderField: "Authorization")
        if let appCheck = auth.appCheckToken, !appCheck.isEmpty {
            request.setValue(appCheck, forHTTPHeaderField: "X-Firebase-AppCheck")
        }
        request.setValue(endpointRequest.requestID, forHTTPHeaderField: "X-Request-Id")
        if let attemptID = endpointRequest.attemptID {
            request.setValue(attemptID, forHTTPHeaderField: "X-Attempt-Id")
        }
        request.setValue(endpointRequest.idempotencyKey, forHTTPHeaderField: "Idempotency-Key")

        let byteStream = transport.open(request)
        defer { byteStream.cancel() }
        var statusCode: Int?
        var contentType: String?
        var body = Data()
        for try await transportEvent in byteStream.events {
            try Task.checkCancellation()
            switch transportEvent {
            case let .response(status, headers):
                statusCode = status
                contentType = headers["content-type"]?.lowercased()
                guard status == 200 else {
                    continue
                }
                guard contentType?.contains("application/json") == true else {
                    throw ParishEndpointError.responseNotJSON
                }
            case let .bytes(bytes):
                guard body.count + bytes.count <= maximumCompletedResponseBytes else {
                    throw ParishEndpointError.responseBodyTooLarge
                }
                body.append(bytes)
            }
        }
        try Task.checkCancellation()
        guard let statusCode else { throw ParishEndpointError.malformedResponse("response metadata is missing") }
        guard !body.isEmpty else { throw ParishEndpointError.malformedResponse("response body is empty") }
        if statusCode != 200 {
            let errorObject = (try? JSONSerialization.jsonObject(with: body)) as? [String: Any]
            let nestedError = errorObject?["error"] as? [String: Any]
            let requestID = (errorObject?["request_id"] as? String)
                ?? (nestedError?["request_id"] as? String)
                ?? endpointRequest.requestID
            let code = (nestedError?["code"] as? String) ?? (errorObject?["code"] as? String)
            let message = (nestedError?["message"] as? String) ?? (errorObject?["message"] as? String)
            throw ParishEndpointError.responseFailure(
                status: statusCode,
                requestID: requestID,
                code: code,
                message: message
            )
        }
        let result = EndpointCompletedResponse(requestID: endpointRequest.requestID, statusCode: statusCode, body: body)
        _ = try result.validatedDialogue()
        return result
    }

    /// Requests cancellation of the matching active stream through the same
    /// authenticated, pinned Endpoint route. This complements transport-level
    /// socket cancellation because an HTTP edge may not propagate a client
    /// disconnect to the serving instance before a fast provider completes.
    public func cancel(_ endpointRequest: EndpointRequest) async throws {
        try Task.checkCancellation()
        try policy.validate(endpointRequest.url)
        guard let attemptID = endpointRequest.attemptID else {
            throw ParishEndpointError.malformedResponse("cancellation attempt identity is missing")
        }
        let auth = try await credentials.credentials()
        try Task.checkCancellation()
        guard !auth.authorizationToken.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw ParishEndpointError.invalidCredential
        }

        var request = URLRequest(url: endpointRequest.url)
        request.httpMethod = "DELETE"
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue("Bearer \(auth.authorizationToken)", forHTTPHeaderField: "Authorization")
        if let appCheck = auth.appCheckToken, !appCheck.isEmpty {
            request.setValue(appCheck, forHTTPHeaderField: "X-Firebase-AppCheck")
        }
        request.setValue(endpointRequest.requestID, forHTTPHeaderField: "X-Request-Id")
        request.setValue(attemptID, forHTTPHeaderField: "X-Attempt-Id")

        let byteStream = transport.open(request)
        defer { byteStream.cancel() }
        var statusCode: Int?
        var bodyBytes = 0
        for try await event in byteStream.events {
            try Task.checkCancellation()
            switch event {
            case let .response(status, _):
                statusCode = status
            case let .bytes(bytes):
                bodyBytes += bytes.count
                guard bodyBytes <= maximumCompletedResponseBytes else {
                    throw ParishEndpointError.responseBodyTooLarge
                }
            }
        }
        try Task.checkCancellation()
        guard let statusCode else {
            throw ParishEndpointError.malformedResponse("cancellation response metadata is missing")
        }
        guard (200..<300).contains(statusCode) else {
            throw ParishEndpointError.responseStatus(statusCode)
        }
    }

    public func stream(_ endpointRequest: EndpointRequest) -> AsyncThrowingStream<EndpointStreamFrame, Error> {
        AsyncThrowingStream(bufferingPolicy: .bufferingOldest(maximumFrames)) { continuation in
            let task = Task {
                do {
                    try Task.checkCancellation()
                    try policy.validate(endpointRequest.url)
                    let auth = try await credentials.credentials()
                    try Task.checkCancellation()
                    guard !auth.authorizationToken.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
                        throw ParishEndpointError.invalidCredential
                    }
                    var request = URLRequest(url: endpointRequest.url)
                    request.httpMethod = "POST"
                    request.httpBody = endpointRequest.body
                    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
                    request.setValue("text/event-stream", forHTTPHeaderField: "Accept")
                    request.setValue("Bearer \(auth.authorizationToken)", forHTTPHeaderField: "Authorization")
                    if let appCheck = auth.appCheckToken, !appCheck.isEmpty {
                        request.setValue(appCheck, forHTTPHeaderField: "X-Firebase-AppCheck")
                    }
                    request.setValue(endpointRequest.requestID, forHTTPHeaderField: "X-Request-Id")
                    if let attemptID = endpointRequest.attemptID {
                        request.setValue(attemptID, forHTTPHeaderField: "X-Attempt-Id")
                    }
                    request.setValue(endpointRequest.idempotencyKey, forHTTPHeaderField: "Idempotency-Key")

                    let byteStream = transport.open(request)
                    defer { byteStream.cancel() }
                    var parser = BoundedSSEParser(limits: limits)
                    var validator = EndpointStreamValidator(
                        expectedRequestID: endpointRequest.requestID,
                        expectedAttemptID: endpointRequest.attemptID,
                        expectedInvocationID: endpointRequest.invocationID,
                        expectedEndpointVersion: endpointRequest.endpointVersion,
                        maximumFrames: maximumFrames
                    )
                    for try await transportEvent in byteStream.events {
                        try Task.checkCancellation()
                        switch transportEvent {
                        case let .response(statusCode, headers):
                            guard (200..<300).contains(statusCode) else { throw ParishEndpointError.responseStatus(statusCode) }
                            guard headers["content-type"]?.lowercased().contains("text/event-stream") == true else {
                                throw ParishEndpointError.responseNotSSE
                            }
                        case let .bytes(bytes):
                            for event in try parser.append(bytes) {
                                let frame = try validator.accept(event)
                                switch continuation.yield(frame) {
                                case .enqueued:
                                    break
                                case .dropped:
                                    throw ParishEndpointError.streamTooLarge
                                case .terminated:
                                    return
                                @unknown default:
                                    throw ParishEndpointError.streamTooLarge
                                }
                            }
                        }
                    }
                    _ = try parser.finish()
                    try validator.finish()
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }
}
