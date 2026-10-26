import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import LimerickEndpointKit
import RundaleSession

/// Answers dialogue calls by replaying a recorded Endpoint stream, such as
/// `mobile/endpoint/fixtures/dialogue-v1.sse`, under the identities of the
/// call being answered. Intent calls go to the scripted mock Endpoint.
public final class SSEFixtureEndpointTransport: EndpointTransport, @unchecked Sendable {
    /// Each recorded event's `data` JSON.
    private let events: [Data]
    private let intent = Phase2MockEndpointTransport()
    private let pauseNanoseconds: UInt64

    /// - Parameter pauseNanoseconds: the wait between replayed events, so a
    ///   frame can show each streamed delta.
    public init(fixture: URL, pauseNanoseconds: UInt64 = 1_000_000_000) throws {
        let text = try String(contentsOf: fixture, encoding: .utf8)
        events = text.components(separatedBy: "\n\n").compactMap { block in
            block.split(separator: "\n").first(where: { $0.hasPrefix("data: ") })
                .map { Data($0.dropFirst("data: ".count).utf8) }
        }
        // A file that is not a whole recorded stream would hang the turn
        // until it timed out; refuse it here instead.
        let types = try events.map { data -> String? in
            guard let event = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                throw SSEFixtureError.notAStream(fixture.path)
            }
            return event["type"] as? String
        }
        guard let last = types.last, last == "final" || last == "error" else {
            throw SSEFixtureError.notAStream(fixture.path)
        }
        self.pauseNanoseconds = pauseNanoseconds
    }

    public func open(_ request: URLRequest) -> EndpointByteStream {
        guard let body = request.httpBody,
              let root = try? JSONSerialization.jsonObject(with: body) as? [String: Any],
              let input = root["input"] as? [String: Any],
              input["role"] as? String != "player_intent",
              let requestID = input["logicalRequestID"] as? String,
              let invocationID = input["idempotencyKey"] as? String,
              let attemptID = request.value(forHTTPHeaderField: "X-Attempt-Id") else {
            return intent.open(request)
        }
        let events = self.events
        let pause = pauseNanoseconds
        let stream = AsyncThrowingStream<EndpointTransportEvent, Error> { continuation in
            let task = Task {
                do {
                    continuation.yield(.response(
                        statusCode: 200, headers: ["content-type": "text/event-stream; charset=utf-8"]
                    ))
                    for (index, data) in events.enumerated() {
                        if index > 0 { try await Task.sleep(nanoseconds: pause) }
                        guard var event = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                            throw LimerickEndpointError.malformedEvent("fixture event is not an object")
                        }
                        let sequence = event["sequence"] as? Int ?? index + 1
                        let eventID = "\(invocationID):\(sequence)"
                        event["request_id"] = requestID
                        event["attempt_id"] = attemptID
                        event["invocation_id"] = invocationID
                        event["event_id"] = eventID
                        let type = event["type"] as? String ?? "progress"
                        var frame = Data("event: \(type)\nid: \(eventID)\ndata: ".utf8)
                        frame.append(try JSONSerialization.data(withJSONObject: event, options: [.sortedKeys]))
                        frame.append(Data("\n\n".utf8))
                        continuation.yield(.bytes(frame))
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
        return EndpointByteStream(events: stream, cancel: {})
    }
}

public enum SSEFixtureError: Error, CustomStringConvertible {
    case notAStream(String)

    public var description: String {
        switch self {
        case let .notAStream(path):
            return "\(path) is not a recorded Endpoint stream ending in a final or error event"
        }
    }
}
