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
}
