import XCTest
import RundaleBridge
import RundaleKit
@testable import Rundale

/// The RundaleKit request-lifecycle tests, run against the real engine
/// through the FFI: the linked Rust library, the bundled canonical world, and
/// a save on disk. The test plays the Endpoint host, answering each pending
/// invocation the way `RundaleEngineController` does, and feeds every event
/// the engine returns into the app's `SessionReducer`.
@MainActor
final class RundaleEngineLifecycleTests: XCTestCase {
    private static let cattleLine = "The wet ground has made moving cattle hard this week."
    private var directory: URL!

    override func setUp() async throws {
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("RundaleEngineLifecycleTests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: directory)
    }

    // RundaleKit `testCompletionCommitsAndLateOldAttemptCannotWin`.
    func testCompletionCommitsAndLateOldAttemptCannotWin() async throws {
        let game = try await Game.open(directory: directory)
        let (request, dialogue) = try await game.talkToMicheal()
        try await game.apply(game.runtime.frame(dialogue, sequence: 1, text: "The wet "))
        try await game.apply(game.runtime.frame(dialogue, sequence: 2, text: "ground"))
        let streamed = try XCTUnwrap(game.state.transcript.last)
        XCTAssertEqual(streamed.content, "The wet ground")
        XCTAssertEqual(streamed.state, .provisional)

        try await game.apply(game.runtime.resolve(dialogue, output: Game.dialogue(Self.cattleLine)))
        let record = try XCTUnwrap(game.state.request(for: request))
        XCTAssertEqual(record.phase, .completed)
        XCTAssertNotNil(record.committedStateRevision)
        let rows = game.state.transcript.filter { $0.id == streamed.id }
        XCTAssertEqual(rows.count, 1, "the committed line replaced the streamed row")
        XCTAssertEqual(rows.first?.content, Self.cattleLine)
        XCTAssertEqual(rows.first?.state, .committed)
        let revision = game.state.stateRevision

        let late = try await game.apply(game.runtime.resolve(dialogue, output: Game.dialogue("A second answer.")))
        XCTAssertTrue(late.isEmpty, "the engine ignores a late result")
        XCTAssertEqual(game.state.stateRevision, revision)
        XCTAssertEqual(game.state.transcript.filter { $0.id == streamed.id }.first?.content, Self.cattleLine)
        try await game.close()
    }

    // RundaleKit `testRetryCreatesCurrentAttemptBeforeRejectingOldAttemptEvents`.
    func testRetryCreatesCurrentAttemptBeforeRejectingOldAttemptEvents() async throws {
        let game = try await Game.open(directory: directory)
        let (request, dialogue) = try await game.talkToMicheal()
        try await game.apply(game.runtime.fail(dialogue, kind: .transport, message: "offline"))
        XCTAssertEqual(game.state.request(for: request)?.phase, .failed)

        let retried = try await game.op(["op": "retry", "logical_request_id": request.rawValue])
        let retryAttempt = ExecutionAttemptID(try XCTUnwrap(retried["attemptID"] as? String))
        XCTAssertEqual(retried["logicalRequestID"] as? String, request.rawValue)
        XCTAssertNotEqual(retryAttempt, dialogue.attemptID)
        let firstEvent = try XCTUnwrap((retried["events"] as? [[String: Any]])?.first)
        XCTAssertEqual(firstEvent["kind"] as? String, "progress")
        XCTAssertEqual(game.state.request(for: request)?.currentAttemptID, retryAttempt,
                       "the retry marker opens the new attempt before any of its output")
        XCTAssertEqual(game.state.request(for: request)?.phase, .executing)
        XCTAssertNil(game.state.request(for: request)?.terminalOutcome)

        let again = try await game.untilDialogue()
        let stale = try await game.apply(game.runtime.resolve(dialogue, output: Game.dialogue("From the old attempt.")))
        XCTAssertTrue(stale.isEmpty, "the old attempt cannot answer the new one")
        XCTAssertEqual(game.state.request(for: request)?.phase, .executing)

        try await game.apply(game.runtime.resolve(again, output: Game.dialogue(Self.cattleLine)))
        XCTAssertEqual(game.state.request(for: request)?.phase, .completed)
        XCTAssertEqual(game.state.request(for: request)?.terminalOutcome, .succeeded)
        try await game.close()
    }

    // RundaleKit `testLateCallbacksFromCancelledAttemptCannotCommit`.
    func testLateCallbacksFromCancelledAttemptCannotCommit() async throws {
        let game = try await Game.open(directory: directory)
        let (request, dialogue) = try await game.talkToMicheal()
        try await game.apply(game.runtime.frame(dialogue, sequence: 1, text: "partial"))
        let row = try XCTUnwrap(game.state.transcript.last)
        let revision = game.state.stateRevision

        let stop = try await game.op(["op": "stop"])
        XCTAssertEqual(stop["terminalOutcome"] as? String, "cancelled")
        XCTAssertEqual(game.state.request(for: request)?.phase, .cancelled)
        XCTAssertEqual(game.state.transcript.first { $0.id == row.id }?.state, .cancelled)

        let lateFrame = try await game.apply(game.runtime.frame(dialogue, sequence: 2, text: " late"))
        let lateResult = try await game.apply(game.runtime.resolve(dialogue, output: Game.dialogue(Self.cattleLine)))
        XCTAssertTrue(lateFrame.isEmpty && lateResult.isEmpty)
        XCTAssertEqual(game.state.request(for: request)?.phase, .cancelled)
        XCTAssertNil(game.state.request(for: request)?.committedStateRevision)
        XCTAssertEqual(game.state.stateRevision, revision)
        XCTAssertEqual(game.state.transcript.first { $0.id == row.id }?.content, "partial")
        try await game.close()
    }

    // RundaleKit `testFailedCompletionCannotAdvanceCommittedStateRevision`.
    func testFailedCompletionCannotAdvanceCommittedStateRevision() async throws {
        let game = try await Game.open(directory: directory)
        let (request, dialogue) = try await game.talkToMicheal()
        let revision = game.state.stateRevision
        let events = try await game.apply(game.runtime.fail(dialogue, kind: .protocolViolation, message: "bad"))
        let terminal = try XCTUnwrap(events.last { $0.kind == .responseCompleted })
        XCTAssertEqual(terminal.terminalOutcome, .failed)
        XCTAssertNil(terminal.stateRevision)
        XCTAssertEqual(game.state.stateRevision, revision)
        XCTAssertEqual(game.state.request(for: request)?.phase, .failed)
        XCTAssertNotNil(game.state.lastError, "the failure is shown to the player")
        try await game.close()
    }

    // RundaleKit `testFixtureClarificationContinuesSameLogicalRequest`.
    func testClarificationContinuesSameLogicalRequest() async throws {
        let world = try Game.worldWithTwoDrovers(in: directory)
        let game = try await Game.open(directory: directory, world: world)
        try await game.goToTheCottage()
        let (request, _) = try await game.submit("Drover, is it a good day for the fair?")
        try await game.answerIntents()
        let pending = try XCTUnwrap(game.state.pendingClarification)
        XCTAssertEqual(pending.requestID, request)
        XCTAssertEqual(pending.prompt.choices.count, 2)
        let roisin = try XCTUnwrap(pending.prompt.choices.first { $0.entityID == "3" })

        let answered = try await game.op([
            "op": "answer_clarification",
            "logical_request_id": request.rawValue,
            "choice_id": roisin.id
        ])
        XCTAssertEqual(answered["logicalRequestID"] as? String, request.rawValue)
        XCTAssertNil(game.state.pendingClarification)
        let dialogue = try await game.untilDialogue()
        XCTAssertEqual(dialogue.logicalRequestID, request)
        try await game.apply(game.runtime.resolve(dialogue, output: Game.dialogue("It is, if the rain holds off.")))
        XCTAssertEqual(game.state.request(for: request)?.phase, .completed)
        try await game.close()
    }

    // RundaleKit `testRestoredAdapterSuppressesOpeningReplayAndInterruptsActiveAttempt`.
    func testRestoredAdapterSuppressesOpeningReplayAndInterruptsActiveAttempt() async throws {
        let first = try await Game.open(directory: directory)
        let opening = try XCTUnwrap(first.state.transcript.first)
        XCTAssertEqual(opening.kind, .sceneChanged)
        XCTAssertEqual(opening.metadata["sceneName"], "Kilteevan Village")
        let (request, _) = try await first.talkToMicheal()
        try await first.close()

        let restored = try await Game.open(directory: directory)
        XCTAssertEqual(restored.state.transcript.filter { $0.content == opening.content }.count, 1,
                       "the opening is not replayed")
        XCTAssertEqual(restored.state.request(for: request)?.phase, .interrupted)
        XCTAssertTrue(restored.state.canSubmit)
        let pending = try await restored.runtime.pendingInvocation()
        XCTAssertNil(pending, "nothing re-runs after restart")
        XCTAssertTrue(restored.state.transcript.contains {
            $0.content == "The previous response was interrupted; you can retry it."
        })

        try await restored.op(["op": "retry", "logical_request_id": request.rawValue])
        let dialogue = try await restored.untilDialogue()
        try await restored.apply(restored.runtime.resolve(dialogue, output: Game.dialogue(Self.cattleLine)))
        XCTAssertEqual(restored.state.request(for: request)?.phase, .completed)
        try await restored.close()
    }
}

/// One engine session and the app reducer it feeds.
@MainActor
private final class Game {
    let runtime: LimerickRuntime
    private var presentation: PresentationSession

    var state: SessionState { presentation.state }

    private init(runtime: LimerickRuntime, presentation: PresentationSession) {
        self.runtime = runtime
        self.presentation = presentation
    }

    static func open(directory: URL, world: URL? = nil) async throws -> Game {
        let mod = try world ?? XCTUnwrap(Bundle.main.url(forResource: "rundale", withExtension: nil, subdirectory: "Mods"),
                                         "the app bundles the canonical world")
        let payload = try JSONSerialization.data(withJSONObject: [
            "save_path": directory.appendingPathComponent("game.sqlite").path,
            "mod_dir": mod.path
        ])
        let runtime = try LimerickRuntime.openResume(payload: payload)
        let opening = try await runtime.openingResponseData()
        let envelope = try XCTUnwrap(JSONSerialization.jsonObject(with: opening) as? [String: Any])
        let snapshot = try XCTUnwrap(envelope["value"] as? [String: Any])
        let sessionID = SessionID(try XCTUnwrap(snapshot["sessionID"] as? String))
        let events = try decode([SemanticEvent].self, snapshot["events"])
        let requests = try decode([RequestRecord].self, snapshot["requests"])
        // Replay the retained tail, then overlay the engine's request
        // projection, as the controller hydrates a restored session.
        let replay = PresentationSession(state: SessionState(sessionID: sessionID))
        for event in events.sorted(by: { $0.sequence < $1.sequence }) { replay.apply(event) }
        let replayed = replay.state
        let hydrated = SessionState(
            sessionID: sessionID,
            stateRevision: StateRevision(try XCTUnwrap(
                (snapshot["stateRevision"] as? [String: Any])?["rawValue"] as? NSNumber
            ).uint64Value),
            eventCursor: replayed.eventCursor,
            transcript: replayed.transcript,
            requests: requests,
            pendingClarification: nil,
            activeRequestID: requests.last { !$0.phase.isTerminal }?.id,
            lastError: replayed.lastError,
            processedEventIDs: replayed.processedEventIDs,
            streamProgress: replayed.streamProgress
        )
        return Game(runtime: runtime, presentation: PresentationSession(state: hydrated))
    }

    private static func decode<T: Decodable>(_ type: T.Type, _ value: Any?) throws -> T {
        try FixtureJSON.decode(type, from: JSONSerialization.data(withJSONObject: value ?? []))
    }

    /// Applies an operation result's events and returns them.
    @discardableResult
    func apply(_ data: Data) throws -> [SemanticEvent] {
        let object = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        let events = try Self.decode([SemanticEvent].self, object?["events"])
        for event in events.sorted(by: { $0.sequence < $1.sequence }) {
            presentation.apply(event)
        }
        return events
    }

    /// Runs one operation, applies its events, and returns its result.
    @discardableResult
    func op(_ operation: [String: Any]) async throws -> [String: Any] {
        let data = try await runtime.dispatchJSON(
            JSONSerialization.data(withJSONObject: operation, options: [.sortedKeys])
        )
        try apply(data)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    func submit(_ text: String) async throws -> (LogicalRequestID, ExecutionAttemptID?) {
        let result = try await op(["op": "submit", "text": text])
        XCTAssertEqual(result["accepted"] as? Bool, true)
        let request = LogicalRequestID(try XCTUnwrap(result["logicalRequestID"] as? String))
        return (request, (result["attemptID"] as? String).map(ExecutionAttemptID.init(rawValue:)))
    }

    func close() async throws {
        try await runtime.close()
    }

    static func dialogue(_ text: String) -> Data {
        (try? JSONSerialization.data(withJSONObject: ["dialogue": text])) ?? Data()
    }

    func goToTheCottage() async throws {
        let (request, _) = try await submit("go to Connolly Cottage")
        XCTAssertEqual(state.request(for: request)?.phase, .completed)
    }

    /// Answers intent calls as speech until the turn waits on something else.
    func answerIntents() async throws {
        while let pending = try await runtime.pendingInvocation(), pending.role == "intent" {
            let output = Data(#"{"intent":"talk","target":null,"dialogue":null,"atmosphere":null}"#.utf8)
            try apply(await runtime.resolve(pending, output: output))
        }
    }

    func untilDialogue() async throws -> LimerickPendingInvocation {
        try await answerIntents()
        let next = try await runtime.pendingInvocation()
        let pending = try XCTUnwrap(next, "a dialogue call is pending")
        XCTAssertTrue(pending.isDialogue)
        return pending
    }

    func talkToMicheal() async throws -> (LogicalRequestID, LimerickPendingInvocation) {
        try await goToTheCottage()
        let (request, _) = try await submit("Mícheál Connolly, how are the cattle this week?")
        return (request, try await untilDialogue())
    }

    /// A copy of the bundled world where Róisín is a cattle drover like
    /// Mícheál, so "Drover" names two people in the cottage.
    static func worldWithTwoDrovers(in directory: URL) throws -> URL {
        let source = try XCTUnwrap(Bundle.main.url(forResource: "rundale", withExtension: nil, subdirectory: "Mods"))
        let copy = directory.appendingPathComponent("rundale", isDirectory: true)
        try FileManager.default.copyItem(at: source, to: copy)
        let npcs = copy.appendingPathComponent("npcs.json")
        let text = try String(contentsOf: npcs, encoding: .utf8)
        try text.replacingOccurrences(of: "Spinner and household bookkeeper", with: "Smallholder and cattle drover")
            .write(to: npcs, atomically: true, encoding: .utf8)
        return copy
    }
}
