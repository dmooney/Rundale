import LimerickMobileFFI
import XCTest
@testable import RundaleBridge

final class LimerickRuntimeTests: XCTestCase {
    func testBoundaryErrorsHaveStableDescriptions() {
        XCTAssertEqual(
            LimerickRuntimeError.closed.errorDescription,
            "The Limerick runtime session is closed."
        )
        XCTAssertEqual(
            LimerickRuntimeError.invalidHandle.errorDescription,
            "The Limerick runtime session is no longer valid."
        )
        XCTAssertEqual(
            LimerickRuntimeError.eventBufferOverflow.errorDescription,
            "The Limerick event stream fell behind; the session was refreshed."
        )
    }

    func testStopSurfacesBridgeDecodeFailure() async throws {
        let runtime = try LimerickRuntime.openNew()
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
        let error = LimerickRuntime.statusError(LIMERICK_MOBILE_INTERNAL_ERROR, response: envelope)
        XCTAssertEqual(
            error,
            .notWired("`open_resume` is not yet wired to the shared Limerick engine (#2044).")
        )
    }

    func testOtherInternalErrorsKeepTheirRawEnvelope() {
        let envelope = Data(#"{"ok":false,"error":{"code":"internal_error","message":"boom"}}"#.utf8)
        let error = LimerickRuntime.statusError(LIMERICK_MOBILE_INTERNAL_ERROR, response: envelope)
        guard case let .internalError(message) = error else {
            return XCTFail("expected internalError, got \(error)")
        }
        XCTAssertTrue(message.contains("boom"))
    }
}
