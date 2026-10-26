import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import LimerickEndpointKit
import RundaleBridge
import RundaleKit
import XCTest
@testable import RundaleSession

/// An engine that answers `RundaleEngineController` from a script: each
/// submission queues the model calls the test names, and the calls the
/// controller makes are recorded in order.
actor ScriptedEngineRuntime: EngineRuntime {
    enum Call: Equatable {
        case submit(String)
        case stop
        case retry(String)
        case answer(String)
        case frame(callID: String, sequence: UInt64, text: String)
        /// `action` is an intent decision's action; `dialogue` a dialogue reply.
        case resolve(callID: String, action: String?, dialogue: String?)
        case fail(callID: String, kind: LimerickRuntimeFailureKind, reason: LimerickFailureReason?)
        case close
    }

    private(set) var calls: [Call] = []
    private(set) var snapshots = 0
    private var pending: [LimerickPendingInvocation] = []
    private var queuedForSubmit: [LimerickPendingInvocation] = []
    private let sessionID = SessionID("scripted-session")

    /// The model calls the next submission waits on, in order.
    func queueForNextSubmit(_ invocations: [LimerickPendingInvocation]) {
        queuedForSubmit = invocations
    }

    func events(after cursor: EventCursor?) async -> AsyncThrowingStream<SemanticEvent, Error> {
        AsyncThrowingStream { $0.finish() }
    }

    func submit(text: String, draftID: DraftID?, logicalRequestID: LogicalRequestID?) async throws -> SubmissionReceipt {
        calls.append(.submit(text))
        pending = queuedForSubmit
        queuedForSubmit = []
        return SubmissionReceipt(
            logicalRequestID: pending.first?.logicalRequestID ?? LogicalRequestID("request"),
            attemptID: pending.first?.attemptID ?? ExecutionAttemptID("attempt"),
            commandEventID: nil,
            accepted: true,
            isRetry: false,
            cursor: EventCursor(0)
        )
    }

    func retry(logicalRequestID: LogicalRequestID) async throws -> SubmissionReceipt {
        calls.append(.retry(logicalRequestID.rawValue))
        return SubmissionReceipt(logicalRequestID: logicalRequestID, attemptID: ExecutionAttemptID("retry"),
                                 commandEventID: nil, accepted: false, isRetry: true, cursor: EventCursor(0))
    }

    func answerClarification(logicalRequestID: LogicalRequestID, choiceID: String) async throws -> SubmissionReceipt {
        calls.append(.answer(choiceID))
        return SubmissionReceipt(logicalRequestID: logicalRequestID, attemptID: ExecutionAttemptID("answer"),
                                 commandEventID: nil, accepted: false, isRetry: false, cursor: EventCursor(0))
    }

    func stop() async throws -> StopReceipt {
        calls.append(.stop)
        pending = []
        return StopReceipt(result: .cancelled)
    }

    func close() async throws {
        calls.append(.close)
    }

    func snapshotJSON() async throws -> Data {
        snapshots += 1
        return try Self.snapshot(sessionID: sessionID)
    }

    func readEventPageBefore(before cursor: EventCursor, limit: Int) async throws -> LimerickEventPage {
        try JSONDecoder().decode(LimerickEventPage.self, from: Data(#"{"events":[]}"#.utf8))
    }

    func bugReport(description: String, build: String?) async throws -> String {
        "Scripted bug report\n\(description)\n"
    }

    func pendingInvocation() async throws -> LimerickPendingInvocation? {
        pending.first
    }

    func resolve(_ invocation: LimerickPendingInvocation, output: Data) async throws -> Data {
        let object = try JSONSerialization.jsonObject(with: output) as? [String: Any] ?? [:]
        calls.append(.resolve(
            callID: invocation.callID,
            action: (object["action"] as? [String: Any])?["choice"] as? String,
            dialogue: object["dialogue"] as? String
        ))
        settle(invocation)
        return Self.noEvents
    }

    func fail(
        _ invocation: LimerickPendingInvocation,
        kind: LimerickRuntimeFailureKind,
        message: String,
        reason: LimerickFailureReason?
    ) async throws -> Data {
        calls.append(.fail(callID: invocation.callID, kind: kind, reason: reason))
        settle(invocation)
        return Self.noEvents
    }

    func frame(_ invocation: LimerickPendingInvocation, sequence: UInt64, text: String) async throws -> Data {
        calls.append(.frame(callID: invocation.callID, sequence: sequence, text: text))
        return Self.noEvents
    }

    /// The engine moves past a call once it has a result for it.
    private func settle(_ invocation: LimerickPendingInvocation) {
        pending.removeAll { $0.callID == invocation.callID }
    }

    private static let noEvents = Data(#"{"events":[]}"#.utf8)

    private static func snapshot(sessionID: SessionID) throws -> Data {
        func json(_ value: some Encodable) throws -> Any {
            try JSONSerialization.jsonObject(with: FixtureJSON.encode(value), options: [.fragmentsAllowed])
        }
        let object: [String: Any] = [
            "contractVersion": try json(PresentationContractVersion.current),
            "sessionID": try json(sessionID),
            "stateRevision": try json(StateRevision(1)),
            "eventCursor": try json(EventCursor(0)),
            "readModel": [
                "scene": ["id": "connolly_cottage", "name": "Connolly Cottage"],
                "nearbyPeople": [["id": "micheal", "displayName": "Mícheál Connolly"]],
                "timeOfDay": "Morning",
                "weather": "Clear",
                "commands": [[String: Any]](),
                "commandCompletions": [[String: Any]](),
                "everyone": [[String: Any]]()
            ],
            "requests": [Any](),
            "events": [Any](),
            "hasOlderEvents": false
        ]
        return try JSONSerialization.data(withJSONObject: object)
    }
}

extension LimerickPendingInvocation {
    /// The intent call that opens a turn: a decision on the JSON route.
    static func intent(line: String, request: String = "request-1", attempt: String = "attempt-1") throws -> Self {
        let callID = "\(attempt)#1"
        return try Self(
            callID: callID, logicalRequestID: LogicalRequestID(request), attemptID: ExecutionAttemptID(attempt),
            baseRevision: StateRevision(1), role: "intent", slug: "rundale-intent", version: 1, streams: false,
            input: JSONSerialization.data(withJSONObject: [
                "role": "player_intent", "logicalRequestID": request, "attemptID": attempt,
                "idempotencyKey": callID, "line": line,
                "present": [["optionId": "micheal", "name": "Mícheál Connolly"]],
                "elsewhere": [Any](), "references": [Any](), "places": [Any](), "phrases": [Any]()
            ])
        )
    }

    /// A dialogue call, streamed on the `/stream` route.
    static func dialogue(line: String, call: Int = 2, request: String = "request-1",
                         attempt: String = "attempt-1") throws -> Self {
        let callID = "\(attempt)#\(call)"
        return try Self(
            callID: callID, logicalRequestID: LogicalRequestID(request), attemptID: ExecutionAttemptID(attempt),
            baseRevision: StateRevision(1), role: "dialogue", slug: "rundale-dialogue", version: 3, streams: true,
            input: JSONSerialization.data(withJSONObject: [
                "role": "dialogue", "logicalRequestID": request, "attemptID": attempt,
                "idempotencyKey": callID, "playerInput": line,
                "speaker": ["id": "micheal", "displayName": "Mícheál Connolly"]
            ])
        )
    }
}

/// The scripted Endpoint the UI suites use, with each request it was asked
/// for recorded. A cancellation (`DELETE`) is answered at once.
final class RecordingEndpointTransport: EndpointTransport, @unchecked Sendable {
    struct Recorded: Equatable {
        let method: String
        let path: String
        let attemptID: String?
        let idempotencyKey: String?
    }

    private let lock = NSLock()
    private var recorded: [Recorded] = []
    private let mock = Phase2MockEndpointTransport()

    var requests: [Recorded] {
        lock.withLock { recorded }
    }

    func open(_ request: URLRequest) -> EndpointByteStream {
        let method = request.httpMethod ?? "GET"
        lock.withLock {
            recorded.append(Recorded(
                method: method,
                path: request.url?.path ?? "",
                attemptID: request.value(forHTTPHeaderField: "X-Attempt-Id"),
                idempotencyKey: request.value(forHTTPHeaderField: "Idempotency-Key")
            ))
        }
        guard method != "DELETE" else {
            let events = AsyncThrowingStream<EndpointTransportEvent, Error> { continuation in
                continuation.yield(.response(statusCode: 202, headers: ["content-type": "application/json"]))
                continuation.yield(.bytes(Data("{}".utf8)))
                continuation.finish()
            }
            return EndpointByteStream(events: events)
        }
        return mock.open(request)
    }
}

/// A launch that plays against a live Endpoint origin, with its save in
/// `directory`.
func liveLaunch(_ directory: URL) -> LaunchConfiguration {
    LaunchConfiguration(
        arguments: ["--ui-tests", "--phase3", "--draft-file=\(directory.appendingPathComponent("projection.json").path)"],
        environment: ["RUNDALE_ENDPOINT_BASE_URL": "https://endpoints.test"],
        bundle: [:]
    )
}

struct StaticCredentials: EndpointCredentialProvider {
    func credentials() async throws -> EndpointCredentials {
        EndpointCredentials(authorizationToken: "test-token", appCheckToken: "test-app-check")
    }
}

/// Waits for `condition`, checking after each turn of the main actor. The
/// ceiling only stops a broken test; a slow machine waits longer, it does not
/// fail.
@MainActor
func waitUntil(
    timeout: TimeInterval = 30,
    _ condition: @MainActor () async -> Bool,
    file: StaticString = #filePath,
    line: UInt = #line
) async {
    let deadline = Date().addingTimeInterval(timeout)
    while Date() < deadline {
        if await condition() { return }
        try? await Task.sleep(nanoseconds: 10_000_000)
    }
    XCTFail("condition did not become true within \(timeout) s", file: file, line: line)
}
