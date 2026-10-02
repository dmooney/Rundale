import LimerickMobileFFI
import RundaleKit
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

    func testEngineRejectionsKeepTheirCodeAndMessage() {
        let envelope = Data(#"""
        {"ok":false,"error":{"code":"request_in_progress","message":"request r1 is still open"}}
        """#.utf8)
        let error = LimerickRuntime.statusError(LIMERICK_MOBILE_PROTOCOL_ERROR, response: envelope)
        XCTAssertEqual(error, .rejected(code: "request_in_progress", message: "request r1 is still open"))
    }

    func testPendingInvocationDecodesTheEngineShape() throws {
        let json = Data(#"""
        {"attemptID":"a1","baseRevision":{"rawValue":3},"callID":"a1#2","endpoint":{"role":"dialogue","slug":"rundale-dialogue","version":1},"input":{"role":"npc_dialogue","sessionID":"s"},"logicalRequestID":"r1","stream":true}
        """#.utf8)
        let pending = try XCTUnwrap(LimerickPendingInvocation.decode(json))
        XCTAssertEqual(pending.callID, "a1#2")
        XCTAssertEqual(pending.baseRevision, StateRevision(3))
        XCTAssertEqual(pending.slug, "rundale-dialogue")
        XCTAssertTrue(pending.isDialogue)
        XCTAssertTrue(pending.streams)
        let body = try JSONSerialization.jsonObject(with: pending.requestBody()) as? [String: Any]
        XCTAssertEqual((body?["input"] as? [String: Any])?["role"] as? String, "npc_dialogue")
        XCTAssertNil(try LimerickPendingInvocation.decode(Data("null".utf8)))
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
