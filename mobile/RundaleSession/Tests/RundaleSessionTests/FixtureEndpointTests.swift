import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import LimerickEndpointKit
import XCTest
@testable import RundaleSession

/// The fixture Endpoint the phase suites play against must answer the
/// engine's `rundale-intent` call: a decision, read from the input's `line`.
/// When it threw instead, the engine absorbed every intent call as unknown
/// input and the fixture suites still passed.
final class FixtureEndpointTests: XCTestCase {
    func testTheIntentCallGetsATalkDecision() async throws {
        let input: [String: Any] = [
            "role": "player_intent",
            "logicalRequestID": "request-1",
            "attemptID": "attempt-1",
            "idempotencyKey": "request-1:attempt-1",
            "line": "Good morning to you",
            "present": [[String: Any]](),
            "elsewhere": [[String: Any]](),
            "references": [[String: Any]](),
            "places": [[String: Any]](),
            "phrases": [[String: Any]](),
        ]
        var request = URLRequest(url: try XCTUnwrap(URL(
            string: "https://example.test/v1/endpoints/limerick-demo/rundale-intent/versions/1"
        )))
        request.httpMethod = "POST"
        request.httpBody = try JSONSerialization.data(withJSONObject: ["input": input])

        var status: Int?
        var body = Data()
        for try await event in Phase2MockEndpointTransport().open(request).events {
            switch event {
            case let .response(statusCode, _): status = statusCode
            case let .bytes(bytes): body.append(bytes)
            }
        }

        XCTAssertEqual(status, 200)
        let output = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        let action = try XCTUnwrap(output["action"] as? [String: Any])
        XCTAssertEqual(action["choice"] as? String, "talk")
        let recipient = try XCTUnwrap(output["recipient"] as? [String: Any])
        XCTAssertEqual(recipient["choice"] as? String, "no_one")
        XCTAssertNotNil(recipient["probabilities"] as? [String: Double])
    }
}
