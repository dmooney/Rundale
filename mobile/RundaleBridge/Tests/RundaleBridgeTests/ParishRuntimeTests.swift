import ParishMobileFFI
import XCTest
@testable import RundaleBridge

final class ParishRuntimeTests: XCTestCase {
    func testBoundaryErrorsHaveStableDescriptions() {
        XCTAssertEqual(
            ParishRuntimeError.closed.errorDescription,
            "The Parish runtime session is closed."
        )
        XCTAssertEqual(
            ParishRuntimeError.invalidHandle.errorDescription,
            "The Parish runtime session is no longer valid."
        )
        XCTAssertEqual(
            ParishRuntimeError.eventBufferOverflow.errorDescription,
            "The Parish event stream fell behind; the session was refreshed."
        )
    }

    func testStopSurfacesBridgeDecodeFailure() async throws {
        let runtime = try ParishRuntime.openNew()
        do {
            _ = try await runtime.stop()
            XCTFail("stop should surface an invalid operation result")
        } catch {
            // The test support bridge intentionally returns an empty value;
            // decoding that result must remain observable to the owner.
            XCTAssertTrue(error is DecodingError)
        }
        try await runtime.close()
    }

    func testNotWiredEnvelopeMapsToDedicatedError() {
        let envelope = Data(#"""
        {"ok":false,"error":{"code":"not_wired","message":"`open_resume` is not yet wired to the shared Limerick engine (#2044).","issue":2044,"engine":{"save_format_version":3}}}
        """#.utf8)
        let error = ParishRuntime.statusError(PARISH_MOBILE_INTERNAL_ERROR, response: envelope)
        XCTAssertEqual(
            error,
            .notWired("`open_resume` is not yet wired to the shared Limerick engine (#2044).")
        )
    }

    func testOtherInternalErrorsKeepTheirRawEnvelope() {
        let envelope = Data(#"{"ok":false,"error":{"code":"internal_error","message":"boom"}}"#.utf8)
        let error = ParishRuntime.statusError(PARISH_MOBILE_INTERNAL_ERROR, response: envelope)
        guard case let .internalError(message) = error else {
            return XCTFail("expected internalError, got \(error)")
        }
        XCTAssertTrue(message.contains("boom"))
    }
}
