import Foundation
import LimerickEndpointKit
import RundaleBridge
import XCTest
@testable import Rundale

/// Which line the player hears for each way a model call can fail
/// (`mods/rundale/loading.toml` `[failure_lines]`), and what recovery that
/// line promises. The classification decides the player's next action, so
/// each Endpoint code and transport error is pinned here, not only through
/// the two mock failures the UI suites drive. The diagnostic is for the log
/// and must never carry anything but a code or a fixed phrase.
@MainActor
final class EndpointFailureClassificationTests: XCTestCase {
    func testEndpointReportedCodesMapToTheirReason() {
        let expected: [(String?, LimerickFailureReason)] = [
            ("AUTHENTICATION_FAILED", .refused),
            ("AUTHORIZATION_FAILED", .refused),
            ("RATE_LIMITED", .busy),
            ("QUOTA_EXCEEDED", .busy),
            ("PROVIDER_RATE_LIMITED", .busy),
            ("PROVIDER_UNAVAILABLE", .unavailable),
            ("INTERNAL_ERROR", .unavailable),
            ("ENDPOINT_NOT_FOUND", .unavailable),
            ("VERSION_NOT_FOUND", .unavailable),
            ("ENDPOINT_DISABLED", .unavailable),
            ("REQUEST_TIMEOUT", .timedOut),
            ("REQUEST_CANCELLED", .cancelled),
            ("OUTPUT_VALIDATION_FAILED", .garbled),
            ("MODEL_ERROR", .garbled),
            ("INVALID_INPUT", .garbled),
            ("A_CODE_THIS_BUILD_DOES_NOT_KNOW", .garbled),
            (nil, .garbled)
        ]
        for (code, reason) in expected {
            let payload = code.map { Data(#"{"code":"\#($0)","message":"x"}"#.utf8) }
            let failure = RundaleEngineController.playerFacingFailure(EndpointReportedFailure(payload: payload))
            XCTAssertEqual(failure.reason, reason, "\(code ?? "no code") in a stream error frame")
            if let code {
                let status = RundaleEngineController.playerFacingFailure(
                    LimerickEndpointError.responseFailure(status: 500, requestID: "r", code: code, message: "x")
                )
                XCTAssertEqual(status.reason, reason, "\(code) in an HTTP error body")
            }
        }
    }

    func testHTTPStatusWithoutACodeMapsToItsReason() {
        let expected: [(Int, LimerickFailureReason)] = [
            (401, .refused), (403, .refused), (429, .busy),
            (500, .unavailable), (503, .unavailable), (504, .unavailable),
            (400, .garbled), (404, .garbled)
        ]
        for (status, reason) in expected {
            let failure = RundaleEngineController.playerFacingFailure(LimerickEndpointError.responseStatus(status))
            XCTAssertEqual(failure.reason, reason, "HTTP \(status)")
        }
    }

    /// Every URL error but the idle timeout is reported as `offline`.
    func testEveryTransportErrorIsReportedAsOffline() {
        for code in [URLError.Code.notConnectedToInternet, .networkConnectionLost, .cannotFindHost,
                     .cannotConnectToHost, .dnsLookupFailed, .secureConnectionFailed] {
            let failure = RundaleEngineController.playerFacingFailure(URLError(code))
            XCTAssertEqual(failure.reason, .offline, "\(code)")
            XCTAssertEqual(failure.kind, .transport, "\(code)")
        }
    }

    /// The connection's idle timeout is a call that ran out of time, reported
    /// as `timed_out`; the engine fails an intent call recoverably for it as
    /// for `offline` (issue 1825).
    func testTheConnectionIdleTimeoutIsReportedAsTimedOut() {
        let failure = RundaleEngineController.playerFacingFailure(URLError(.timedOut))
        XCTAssertEqual(failure.reason, .timedOut)
        XCTAssertEqual(failure.kind, .timedOut)
    }

    func testAStreamThatEndsEarlyOrBreaksItsProtocolIsNotAPlainOutage() {
        let early = RundaleEngineController.playerFacingFailure(LimerickEndpointError.missingTerminal)
        XCTAssertEqual(early.reason, .offline)
        XCTAssertEqual(early.kind, .missingTerminal)
        let tangled = RundaleEngineController.playerFacingFailure(LimerickEndpointError.malformedEvent("x"))
        XCTAssertEqual(tangled.reason, .garbled)
        let credential = RundaleEngineController.playerFacingFailure(LimerickEndpointError.invalidCredential)
        XCTAssertEqual(credential.reason, .refused)
    }

    func testDiagnosticsNeverCarryTheServerMessageOrABody() {
        let secretMessage = "sk-secret provider body Bearer abc"
        let failure = RundaleEngineController.playerFacingFailure(
            LimerickEndpointError.responseFailure(
                status: 502, requestID: "r", code: "MODEL_ERROR", message: secretMessage
            )
        )
        XCTAssertFalse(failure.diagnostic.contains("secret"))
        XCTAssertFalse(failure.diagnostic.contains("Bearer"))
    }
}
