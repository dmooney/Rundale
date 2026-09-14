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
}
