import Foundation
import XCTest
@testable import ParishEndpointKit

final class EndpointKitTests: XCTestCase {
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
