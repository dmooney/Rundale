import Foundation
import XCTest
@testable import ParishEndpointKit

final class EndpointKitTests: XCTestCase {
    private struct CompletedMockTransport: EndpointTransport {
        let status: Int
        let contentType: String
        let body: Data
        let delay: UInt64

        func open(_ request: URLRequest) -> EndpointByteStream {
            let events = AsyncThrowingStream<EndpointTransportEvent, Error> { continuation in
                Task {
                    do {
                        if delay > 0 { try await Task.sleep(nanoseconds: delay) }
                        continuation.yield(.response(statusCode: status, headers: ["content-type": contentType]))
                        continuation.yield(.bytes(body))
                        continuation.finish()
                    } catch {
                        continuation.finish(throwing: error)
                    }
                }
            }
            return EndpointByteStream(events: events)
        }
    }

    private final class RecordingCompletedTransport: EndpointTransport, @unchecked Sendable {
        private let response: Data
        private(set) var request: URLRequest?

        init(response: Data) { self.response = response }

        func open(_ request: URLRequest) -> EndpointByteStream {
            self.request = request
            let events = AsyncThrowingStream<EndpointTransportEvent, Error> { continuation in
                continuation.yield(.response(statusCode: 200, headers: ["content-type": "application/json"]))
                continuation.yield(.bytes(self.response))
                continuation.finish()
            }
            return EndpointByteStream(events: events)
        }
    }

    private final class CancellationRecordingTransport: EndpointTransport, @unchecked Sendable {
        private var cancelled = false
        private var opened = false
        private let lock = NSLock()

        func open(_ request: URLRequest) -> EndpointByteStream {
            lock.lock()
            opened = true
            lock.unlock()
            let events = AsyncThrowingStream<EndpointTransportEvent, Error> { continuation in
                continuation.yield(.response(statusCode: 200, headers: ["content-type": "text/event-stream"]))
            }
            return EndpointByteStream(events: events) { [weak self] in
                self?.lock.lock()
                self?.cancelled = true
                self?.lock.unlock()
            }
        }

        func wasCancelled() -> Bool {
            lock.lock(); defer { lock.unlock() }
            return cancelled
        }

        func wasOpened() -> Bool {
            lock.lock(); defer { lock.unlock() }
            return opened
        }
    }

    /// Models Firebase callbacks that may finish after their calling Swift
    /// task has already been cancelled.
    private final class DeferredCredentialProvider: EndpointCredentialProvider, @unchecked Sendable {
        private let lock = NSLock()
        private var continuation: CheckedContinuation<EndpointCredentials, any Error>?

        func credentials() async throws -> EndpointCredentials {
            try await withCheckedThrowingContinuation { continuation in
                lock.lock()
                self.continuation = continuation
                lock.unlock()
            }
        }

        func hasWaiter() -> Bool {
            lock.lock(); defer { lock.unlock() }
            return continuation != nil
        }

        func resume() {
            lock.lock()
            let continuation = continuation
            self.continuation = nil
            lock.unlock()
            continuation?.resume(returning: .init(authorizationToken: "late-token"))
        }
    }

    private func completedRequest() throws -> EndpointRequest {
        try EndpointRequest(
            url: URL(string: "https://endpoint.example.test/dialogue")!,
            requestID: "request-1",
            attemptID: "attempt-1",
            body: Data(#"{"input":{"playerInput":"hello"}}"#.utf8)
        )
    }

    func testCompletedResponseReturnsStrictJSONAndRequestIdentity() async throws {
        let body = Data(#"{"dialogue":"The rain has eased."}"#.utf8)
        let client = ParishEndpointClient(
            credentials: StaticEndpointCredentialProvider(.init(authorizationToken: "firebase-id-token")),
            transport: CompletedMockTransport(status: 200, contentType: "application/json; charset=utf-8", body: body, delay: 0)
        )

        let response = try await client.complete(try completedRequest())

        XCTAssertEqual(response.requestID, "request-1")
        XCTAssertEqual(response.statusCode, 200)
        XCTAssertEqual(try response.validatedDialogue(), "The rain has eased.")
    }

    func testCompletedRequestUsesJSONHeadersAndCorrelation() async throws {
        let transport = RecordingCompletedTransport(response: Data(#"{"dialogue":"ok"}"#.utf8))
        let client = ParishEndpointClient(
            credentials: StaticEndpointCredentialProvider(.init(authorizationToken: "firebase-id-token", appCheckToken: "app-check")),
            transport: transport
        )
        _ = try await client.complete(try completedRequest())
        let request = try XCTUnwrap(transport.request)
        XCTAssertEqual(request.httpMethod, "POST")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Accept"), "application/json")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Content-Type"), "application/json")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Authorization"), "Bearer firebase-id-token")
        XCTAssertEqual(request.value(forHTTPHeaderField: "X-Firebase-AppCheck"), "app-check")
        XCTAssertEqual(request.value(forHTTPHeaderField: "X-Request-Id"), "request-1")
        XCTAssertEqual(request.value(forHTTPHeaderField: "X-Attempt-Id"), "attempt-1")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Idempotency-Key"), "request-1")
        XCTAssertEqual(request.httpBody, Data(#"{"input":{"playerInput":"hello"}}"#.utf8))
    }

    func testCancellationUsesAuthenticatedCorrelationWithoutRequestBody() async throws {
        let transport = RecordingCompletedTransport(response: Data(#"{"status":"cancellation_requested"}"#.utf8))
        let client = ParishEndpointClient(
            credentials: StaticEndpointCredentialProvider(.init(authorizationToken: "firebase-id-token", appCheckToken: "app-check")),
            transport: transport
        )

        try await client.cancel(try completedRequest())

        let request = try XCTUnwrap(transport.request)
        XCTAssertEqual(request.httpMethod, "DELETE")
        XCTAssertNil(request.httpBody)
        XCTAssertEqual(request.value(forHTTPHeaderField: "Accept"), "application/json")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Authorization"), "Bearer firebase-id-token")
        XCTAssertEqual(request.value(forHTTPHeaderField: "X-Firebase-AppCheck"), "app-check")
        XCTAssertEqual(request.value(forHTTPHeaderField: "X-Request-Id"), "request-1")
        XCTAssertEqual(request.value(forHTTPHeaderField: "X-Attempt-Id"), "attempt-1")
    }

    func testCompletedResponseRejectsStatusContentTypeAndMalformedJSON() async throws {
        let credentials = StaticEndpointCredentialProvider(.init(authorizationToken: "firebase-id-token"))
        let request = try completedRequest()

        do {
            _ = try await ParishEndpointClient(
                credentials: credentials,
                transport: CompletedMockTransport(status: 502, contentType: "application/json", body: Data(#"{"error":{"code":"upstream_unavailable","message":"temporary","request_id":"server-r"}}"#.utf8), delay: 0)
            ).complete(request)
            XCTFail("HTTP errors must be surfaced without decoding output")
        } catch {
            XCTAssertEqual(
                error as? ParishEndpointError,
                .responseFailure(status: 502, requestID: "server-r", code: "upstream_unavailable", message: "temporary")
            )
        }

        do {
            _ = try await ParishEndpointClient(
                credentials: credentials,
                transport: CompletedMockTransport(status: 200, contentType: "text/plain", body: Data("ok".utf8), delay: 0)
            ).complete(request)
            XCTFail("non-JSON responses must be rejected")
        } catch { XCTAssertEqual(error as? ParishEndpointError, .responseNotJSON) }

        do {
            _ = try await ParishEndpointClient(
                credentials: credentials,
                transport: CompletedMockTransport(status: 200, contentType: "application/json", body: Data("[]".utf8), delay: 0)
            ).complete(request)
            XCTFail("schema output must be a JSON object")
        } catch { XCTAssertEqual(error as? ParishEndpointError, .malformedResponse("output is not a JSON object")) }
    }

    func testCompletedDialogueRejectsMissingExtraAndOversizeFields() async throws {
        let credentials = StaticEndpointCredentialProvider(.init(authorizationToken: "firebase-id-token"))
        let outputs = [
            Data(#"{}"#.utf8),
            Data(#"{"dialogue":"ok","extra":true}"#.utf8),
            Data(#"{"dialogue":""}"#.utf8),
            Data((#"{"dialogue":""# + String(repeating: "x", count: 8_193) + #""}"#).utf8)
        ]
        for body in outputs {
            do {
                _ = try await ParishEndpointClient(
                    credentials: credentials,
                    transport: CompletedMockTransport(status: 200, contentType: "application/json", body: body, delay: 0)
                ).complete(try completedRequest())
                XCTFail("invalid dialogue output must be rejected")
            } catch {
                XCTAssertEqual(error as? ParishEndpointError, .malformedResponse("output does not match the dialogue schema"))
            }
        }
    }

    func testCompletedResponseCancellationDoesNotRetry() async throws {
        let client = ParishEndpointClient(
            credentials: StaticEndpointCredentialProvider(.init(authorizationToken: "firebase-id-token")),
            transport: CompletedMockTransport(status: 200, contentType: "application/json", body: Data(#"{"dialogue":"late"}"#.utf8), delay: 2_000_000_000)
        )
        let request = try completedRequest()
        let task = Task { try await client.complete(request) }
        task.cancel()
        do {
            _ = try await task.value
            XCTFail("cancelled completed requests must not produce a result")
        } catch is CancellationError {
            // Cancellation is terminal; the client deliberately does not retry.
        }
    }

    private func event(
        type: String,
        sequence: Int,
        requestID: String = "r",
        attemptID: String = "a",
        invocationID: String = "i",
        eventID: String? = nil,
        output: [String: Any]? = nil,
        error: [String: Any]? = nil,
        text: String? = nil
    ) throws -> SSEEvent {
        let resolvedEventID = eventID ?? "\(invocationID):\(sequence)"
        var object: [String: Any] = [
            "contract_version": 1,
            "request_id": requestID,
            "attempt_id": attemptID,
            "invocation_id": invocationID,
            "event_id": resolvedEventID,
            "sequence": sequence,
            "type": type,
            "endpoint_version": 1
        ]
        if let output { object["output"] = output }
        if let error { object["error"] = error }
        if let text { object["text"] = text }
        return SSEEvent(
            event: type,
            id: resolvedEventID,
            data: try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
        )
    }

    func testParserHandlesChunkBoundariesAndCRLF() throws {
        var parser = BoundedSSEParser()
        XCTAssertTrue(try parser.append(Data("event: progress\r\nda".utf8)).isEmpty)
        let events = try parser.append(Data("ta: {\"contract_version\":1}\r\n\r\n".utf8))
        XCTAssertEqual(events.count, 1)
        XCTAssertEqual(events[0].event, "progress")
        XCTAssertTrue(try parser.finish().isEmpty)
    }

    func testVersionedRepositoryFixtureParsesWithFragmentedUTF8Chunks() throws {
        let fixtureURL = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("endpoint/fixtures/dialogue-v1.sse")
        let fixture = try Data(contentsOf: fixtureURL)
        let emojiOffset = try XCTUnwrap(fixture.firstIndex(of: 0xF0))
        XCTAssertGreaterThan(emojiOffset % 11, 7, "fixture emoji must cross a transport chunk boundary")
        var parser = BoundedSSEParser()
        var validator = EndpointStreamValidator(
            expectedRequestID: "request-fixture",
            expectedAttemptID: "attempt-fixture",
            expectedInvocationID: "invocation-fixture"
        )
        var frames: [EndpointStreamFrame] = []
        for index in stride(from: 0, to: fixture.count, by: 11) {
            let end = min(index + 11, fixture.count)
            for event in try parser.append(fixture[index..<end]) {
                frames.append(try validator.accept(event))
            }
        }
        _ = try parser.finish()
        try validator.finish()
        XCTAssertEqual(frames.map(\.kind), [.progress, .textDelta, .textDelta, .final])
        XCTAssertEqual(frames.last?.payload.flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }?["dialogue"] as? String, "The rain keeps the old road quiet. 🌧")
    }

    func testStreamCancellationCallsUnderlyingTransportCancel() async throws {
        let transport = CancellationRecordingTransport()
        let client = ParishEndpointClient(
            credentials: StaticEndpointCredentialProvider(.init(authorizationToken: "token")),
            transport: transport
        )
        let request = try EndpointRequest(
            url: URL(string: "https://endpoint.example.test/v1/endpoints/rundale/rundale-dialogue/versions/1/stream")!,
            requestID: "r", attemptID: "a", invocationID: "i", body: Data("{}".utf8)
        )
        let task = Task {
            for try await _ in client.stream(request) { }
        }
        try await Task.sleep(nanoseconds: 50_000_000)
        task.cancel()
        _ = await task.result
        for _ in 0..<20 where !transport.wasCancelled() {
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        XCTAssertTrue(transport.wasCancelled())
    }

    func testStreamCancelledDuringCredentialCallbackNeverStartsTransport() async throws {
        let credentials = DeferredCredentialProvider()
        let transport = CancellationRecordingTransport()
        let client = ParishEndpointClient(credentials: credentials, transport: transport)
        let request = try EndpointRequest(
            url: URL(string: "https://endpoint.example.test/v1/endpoints/rundale/rundale-dialogue/versions/1/stream")!,
            requestID: "r", attemptID: "a", invocationID: "i", body: Data("{}".utf8)
        )
        let task = Task {
            for try await _ in client.stream(request) { }
        }
        for _ in 0..<20 where !credentials.hasWaiter() {
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        XCTAssertTrue(credentials.hasWaiter())

        task.cancel()
        credentials.resume()
        _ = await task.result

        XCTAssertFalse(transport.wasOpened())
    }

    func testValidatorRejectsCrossCorrelationAndBadOrder() throws {
        var validator = EndpointStreamValidator(expectedRequestID: "r", expectedAttemptID: "a")
        _ = try validator.accept(try event(type: "text_delta", sequence: 1, text: "a"))
        XCTAssertThrowsError(try validator.accept(try event(type: "final", sequence: 3, output: ["dialogue": "ab"]))) { error in
            XCTAssertEqual(error as? ParishEndpointError, .outOfOrderSequence(expected: 2, actual: 3))
        }

        var other = EndpointStreamValidator(expectedRequestID: "r", expectedAttemptID: "a")
        XCTAssertThrowsError(try other.accept(try event(type: "text_delta", sequence: 1, requestID: "other", text: "a"))) { error in
            XCTAssertEqual(error as? ParishEndpointError, .crossCorrelation(expected: "r", actual: "other"))
        }
    }

    func testValidatorPinsServerInvocationIDAcrossFrames() throws {
        var validator = EndpointStreamValidator(expectedRequestID: "r", expectedAttemptID: "a")
        _ = try validator.accept(try event(type: "text_delta", sequence: 1, text: "a"))
        XCTAssertThrowsError(try validator.accept(try event(
            type: "final",
            sequence: 2,
            invocationID: "other",
            output: ["dialogue": "ab"]
        ))) { error in
            XCTAssertEqual(error as? ParishEndpointError, .crossCorrelation(expected: "i", actual: "other"))
        }
    }

    func testValidatorRejectsMissingTerminalDuplicateAndTruncated() throws {
        var validator = EndpointStreamValidator(expectedRequestID: "r", expectedAttemptID: "a")
        _ = try validator.accept(try event(type: "text_delta", sequence: 1, text: "a"))
        XCTAssertThrowsError(try validator.finish()) { error in
            XCTAssertEqual(error as? ParishEndpointError, .missingTerminal)
        }

        _ = try validator.accept(try event(type: "final", sequence: 2, output: ["dialogue": "ab"]))
        XCTAssertThrowsError(try validator.accept(try event(type: "final", sequence: 3, output: ["dialogue": "abc"]))) { error in
            XCTAssertEqual(error as? ParishEndpointError, .duplicateTerminal)
        }
        try validator.finish()

        var parser = BoundedSSEParser()
        _ = try parser.append(Data("event: text_delta\ndata: {\"x\":1}".utf8))
        XCTAssertThrowsError(try parser.finish()) { error in
            XCTAssertEqual(error as? ParishEndpointError, .truncatedEvent)
        }
    }

    func testFrozenWireRequiresMatchingTypeAndValidatedFinalOutput() throws {
        var validator = EndpointStreamValidator(expectedRequestID: "r", expectedAttemptID: "a")
        let mismatched = SSEEvent(
            event: "progress",
            id: "i:1",
            data: try JSONSerialization.data(withJSONObject: [
                "contract_version": 1, "request_id": "r", "attempt_id": "a",
                "invocation_id": "i", "event_id": "i:1", "sequence": 1,
                "type": "text_delta", "endpoint_version": 1, "text": "x"
            ])
        )
        XCTAssertThrowsError(try validator.accept(mismatched))

        let missingDialogue = try event(type: "final", sequence: 1, output: ["text": "raw"])
        XCTAssertThrowsError(try validator.accept(missingDialogue))

        let scalarEventID = SSEEvent(
            event: "progress",
            id: "i:1",
            data: try JSONSerialization.data(withJSONObject: [
                "contract_version": 1, "request_id": "r", "attempt_id": "a",
                "invocation_id": "i", "event_id": "i:other", "sequence": 1,
                "type": "progress", "endpoint_version": 1
            ])
        )
        XCTAssertThrowsError(try validator.accept(scalarEventID))

        var extraOutputValidator = EndpointStreamValidator(expectedRequestID: "r", expectedAttemptID: "a")
        let extraOutput = try event(type: "final", sequence: 1, output: ["dialogue": "ok", "extra": true])
        XCTAssertThrowsError(try extraOutputValidator.accept(extraOutput))

        var emptyOutputValidator = EndpointStreamValidator(expectedRequestID: "r", expectedAttemptID: "a")
        let emptyOutput = try event(type: "final", sequence: 1, output: ["dialogue": ""])
        XCTAssertThrowsError(try emptyOutputValidator.accept(emptyOutput))
    }

    func testValidatorRejectsBooleanFractionalAndWrongTerminalTypes() throws {
        func rawEvent(
            type: String = "text_delta",
            sequence: Any = 1,
            contractVersion: Any = 1,
            terminal: Any? = nil
        ) throws -> SSEEvent {
            var object: [String: Any] = [
                "contract_version": contractVersion,
                "request_id": "r",
                "attempt_id": "a",
                "invocation_id": "i",
                "event_id": "i:1",
                "sequence": sequence,
                "type": type,
                "endpoint_version": 1,
                "text": "x"
            ]
            if let terminal { object["terminal"] = terminal }
            return SSEEvent(
                event: type,
                id: "i:1",
                data: try JSONSerialization.data(withJSONObject: object)
            )
        }

        for sequence in [true as Any, 1.5 as Any] {
            var validator = EndpointStreamValidator(expectedRequestID: "r", expectedAttemptID: "a")
            XCTAssertThrowsError(try validator.accept(try rawEvent(sequence: sequence)))
        }
        for version in [true as Any, 1.5 as Any] {
            var validator = EndpointStreamValidator(expectedRequestID: "r", expectedAttemptID: "a")
            XCTAssertThrowsError(try validator.accept(try rawEvent(contractVersion: version)))
        }
        for terminal in [1 as Any, 1.0 as Any, "true" as Any] {
            var validator = EndpointStreamValidator(expectedRequestID: "r", expectedAttemptID: "a")
            XCTAssertThrowsError(try validator.accept(try rawEvent(terminal: terminal)))
        }
    }

    func testOversizeAndURLPolicy() throws {
        var parser = BoundedSSEParser(limits: .init(maximumLineBytes: 4, maximumEventBytes: 8))
        XCTAssertThrowsError(try parser.append(Data("data: too-long\n".utf8))) { error in
            XCTAssertEqual(error as? ParishEndpointError, .lineTooLarge)
        }
        var chunkedParser = BoundedSSEParser(limits: .init(maximumLineBytes: 16, maximumEventBytes: 32))
        XCTAssertThrowsError(try chunkedParser.append(Data(repeating: 0x61, count: 2 * 1024 * 1024))) { error in
            XCTAssertEqual(error as? ParishEndpointError, .lineTooLarge)
        }
        XCTAssertEqual(
            EndpointStreamValidator(
                expectedRequestID: "r",
                maximumFrames: Int.max
            ).maximumFrames,
            EndpointResourceLimits.maximumFrames
        )
        XCTAssertThrowsError(try EndpointRequest(url: URL(string: "http://example.com")!, requestID: "r", body: Data()))
        XCTAssertThrowsError(try EndpointRequest(
            url: URL(string: "https://example.com")!,
            requestID: "r",
            body: Data(repeating: 0, count: EndpointResourceLimits.maximumRequestBodyBytes + 1)
        )) { error in
            XCTAssertEqual(error as? ParishEndpointError, .requestTooLarge)
        }
        XCTAssertNoThrow(try EndpointRequest(
            url: URL(string: "http://127.0.0.1:1234")!,
            requestID: "r",
            policy: EndpointURLPolicy(allowLoopbackHTTP: true),
            body: Data()
        ))
    }
}
