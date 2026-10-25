import Foundation
import LimerickEndpointKit
import RundaleBridge
import RundaleKit
import XCTest
@testable import RundaleSession

/// The inference loop `RundaleEngineController` runs for each model call the
/// engine waits on: which Endpoint route it calls, what it sends, and what
/// it hands back to the engine. The engine is scripted; the Endpoint is the
/// mock the phase UI suites play against.
@MainActor
final class EngineControllerInferenceTests: XCTestCase {
    private static let cattle = ["The wet ground ", "has made moving cattle ", "difficult this week."]
    private var directory: URL!

    override func setUp() async throws {
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("EngineControllerInferenceTests-\(UUID().uuidString)")
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: directory)
    }

    private func started(
        _ runtime: ScriptedEngineRuntime,
        transport: RecordingEndpointTransport
    ) async -> RundaleEngineController {
        let controller = RundaleEngineController(
            configuration: liveLaunch(directory),
            credentials: { StaticCredentials() },
            modDirectory: nil,
            log: { _ in },
            transport: transport,
            openRuntime: { _ in runtime }
        )
        controller.start()
        await waitUntil { controller.state.sessionID == SessionID("scripted-session") }
        return controller
    }

    func testATurnDecidesOnTheJSONRouteThenStreamsTheReplyFrameByFrame() async throws {
        let runtime = ScriptedEngineRuntime()
        let transport = RecordingEndpointTransport()
        let controller = await started(runtime, transport: transport)
        let intent = try LimerickPendingInvocation.intent(line: "Mícheál, how are the cattle?")
        let dialogue = try LimerickPendingInvocation.dialogue(line: "Mícheál, how are the cattle?")
        await runtime.queueForNextSubmit([intent, dialogue])

        let receipt = try await controller.submit("Mícheál, how are the cattle?")
        XCTAssertTrue(receipt.accepted)
        await waitUntil { await runtime.calls.count == 6 }

        let calls = await runtime.calls
        XCTAssertEqual(calls, [
            .submit("Mícheál, how are the cattle?"),
            .resolve(callID: intent.callID, action: "talk", dialogue: nil),
            .frame(callID: dialogue.callID, sequence: 1, text: Self.cattle[0]),
            .frame(callID: dialogue.callID, sequence: 2, text: Self.cattle[1]),
            .frame(callID: dialogue.callID, sequence: 3, text: Self.cattle[2]),
            .resolve(callID: dialogue.callID, action: nil, dialogue: Self.cattle.joined())
        ])
        // The intent Endpoint does not stream, so it is asked on the JSON
        // route. Each call sends its own attempt id ("#" made "."): reusing
        // one fails the second stream with HTTP 500 (mobile/AGENTS.md).
        XCTAssertEqual(transport.requests, [
            .init(method: "POST", path: "/v1/endpoints/rundale/rundale-intent/versions/1",
                  attemptID: "attempt-1.1", idempotencyKey: "attempt-1#1"),
            .init(method: "POST", path: "/v1/endpoints/rundale/rundale-dialogue/versions/3/stream",
                  attemptID: "attempt-1.2", idempotencyKey: "attempt-1#2")
        ])
        XCTAssertNil(controller.persistenceError)
    }

    func testAnEndpointErrorEndsTheCallWithItsReason() async throws {
        let runtime = ScriptedEngineRuntime()
        let controller = await started(runtime, transport: RecordingEndpointTransport())
        let dialogue = try LimerickPendingInvocation.dialogue(line: "this will fail")
        await runtime.queueForNextSubmit([dialogue])

        _ = try await controller.submit("this will fail")
        await waitUntil { await runtime.calls.count == 2 }

        let calls = await runtime.calls
        XCTAssertEqual(calls.last, .fail(callID: dialogue.callID, kind: .transport, reason: .unavailable))
    }

    func testARateLimitedDecisionFailsAsBusy() async throws {
        let runtime = ScriptedEngineRuntime()
        let controller = await started(runtime, transport: RecordingEndpointTransport())
        let intent = try LimerickPendingInvocation.intent(line: "busy once")
        await runtime.queueForNextSubmit([intent])

        _ = try await controller.submit("busy once")
        await waitUntil { await runtime.calls.count == 2 }

        let calls = await runtime.calls
        XCTAssertEqual(calls.last, .fail(callID: intent.callID, kind: .transport, reason: .busy))
    }

    func testALostConnectionFailsAsOffline() async throws {
        let runtime = ScriptedEngineRuntime()
        let controller = await started(runtime, transport: RecordingEndpointTransport())
        let dialogue = try LimerickPendingInvocation.dialogue(line: "offline once")
        await runtime.queueForNextSubmit([dialogue])

        _ = try await controller.submit("offline once")
        await waitUntil { await runtime.calls.count == 2 }

        let calls = await runtime.calls
        XCTAssertEqual(calls.last, .fail(callID: dialogue.callID, kind: .transport, reason: .offline))
    }

    func testStopCancelsTheOpenStreamOnTheServerAndStopsTheEngine() async throws {
        let runtime = ScriptedEngineRuntime()
        let transport = RecordingEndpointTransport()
        let controller = await started(runtime, transport: transport)
        let dialogue = try LimerickPendingInvocation.dialogue(line: "a slow answer")
        await runtime.queueForNextSubmit([dialogue])

        _ = try await controller.submit("a slow answer")
        await waitUntil { await runtime.calls.count == 2 }
        let receipt = try await controller.stop()

        XCTAssertEqual(receipt.result, .cancelled)
        let calls = await runtime.calls
        XCTAssertEqual(calls, [
            .submit("a slow answer"),
            .frame(callID: dialogue.callID, sequence: 1, text: Self.cattle[0]),
            .stop
        ])
        // The server keys the stream by request and attempt, so the cancel
        // names the call's own attempt id on the same route.
        XCTAssertEqual(transport.requests.last, .init(
            method: "DELETE", path: "/v1/endpoints/rundale/rundale-dialogue/versions/3/stream",
            attemptID: "attempt-1.2", idempotencyKey: nil
        ))
    }

    func testTheMockLaunchMakesNoCredentialsAndCallsTheLoopbackMock() async throws {
        let runtime = ScriptedEngineRuntime()
        let controller = RundaleEngineController(
            configuration: LaunchConfiguration(
                arguments: ["--ui-tests", "--phase3", "--phase3-mock",
                            "--draft-file=\(directory.appendingPathComponent("projection.json").path)"],
                environment: [:], bundle: [:]
            ),
            credentials: {
                XCTFail("a mock launch must not make live credentials")
                return StaticCredentials()
            },
            modDirectory: nil,
            log: { _ in },
            openRuntime: { _ in runtime }
        )
        controller.start()
        await waitUntil { controller.state.sessionID == SessionID("scripted-session") }
        let intent = try LimerickPendingInvocation.intent(line: "busy once")
        await runtime.queueForNextSubmit([intent])

        _ = try await controller.submit("busy once")
        await waitUntil { await runtime.calls.count == 2 }

        let calls = await runtime.calls
        XCTAssertEqual(calls.last, .fail(callID: intent.callID, kind: .transport, reason: .busy))
    }

    func testTheOpenPayloadNamesTheSaveBesideTheProjectionAndTheMod() async throws {
        let payloads = PayloadBox()
        let mod = directory.appendingPathComponent("Mods/rundale")
        let controller = RundaleEngineController(
            configuration: liveLaunch(directory),
            credentials: { StaticCredentials() },
            modDirectory: mod,
            log: { _ in },
            openRuntime: { payload in
                payloads.append(payload)
                return ScriptedEngineRuntime()
            }
        )
        controller.start()
        await waitUntil { controller.state.sessionID == SessionID("scripted-session") }

        let payload = try XCTUnwrap(payloads.first)
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: payload) as? [String: String])
        XCTAssertEqual(object["save_path"], directory.appendingPathComponent("phase2.sqlite").path)
        XCTAssertEqual(object["mod_dir"], mod.path)
    }

    func testAnEngineThatCannotOpenSaysWhyWithoutAStateChange() async {
        let controller = RundaleEngineController(
            configuration: liveLaunch(directory),
            credentials: { StaticCredentials() },
            modDirectory: nil,
            log: { _ in },
            openRuntime: { _ in
                throw LimerickRuntimeError.rejected(code: "save_locked", message: "The game is open elsewhere.")
            }
        )
        let presenter = RundaleSessionPresenter(
            launch: liveLaunch(directory), session: controller, bugReports: nil,
            captureScreenshot: { nil }, deviceDescription: "test"
        )
        presenter.start()

        await waitUntil { presenter.submissionMessage != nil }
        XCTAssertEqual(presenter.submissionMessage, "The game is open elsewhere.")
        XCTAssertNotNil(controller.persistenceDiagnostic)
    }
}

private final class PayloadBox: @unchecked Sendable {
    private let lock = NSLock()
    private var payloads: [Data] = []

    var first: Data? { lock.withLock { payloads.first } }

    func append(_ payload: Data) {
        lock.withLock { payloads.append(payload) }
    }
}
