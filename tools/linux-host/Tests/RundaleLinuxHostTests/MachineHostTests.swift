import Foundation
import RundaleLinuxHostCore
import XCTest

@MainActor
final class MachineHostTests: XCTestCase {
    private static let repositoryRoot = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()

    func testInitializeAndRejectedNewGameLeaveExistingSaveUntouched() async throws {
        let directory = try makeSaveDirectory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let sentinel = directory.appendingPathComponent("projection.json")
        let original = Data("existing committed save".utf8)
        try original.write(to: sentinel)
        let host = makeHost(directory)

        _ = try await host.command("initialize")
        XCTAssertFalse(FileManager.default.fileExists(atPath: directory.appendingPathComponent(".session-host.lock").path))
        do {
            _ = try await host.command("new_game", args: ["replaceExisting": false])
            XCTFail("Expected existing save to be protected")
        } catch {
            XCTAssertEqual((error as? HostCommandError)?.rawValue, HostCommandError.existingSave.rawValue)
        }
        XCTAssertEqual(try Data(contentsOf: sentinel), original)
        XCTAssertEqual(
            Set(try FileManager.default.contentsOfDirectory(atPath: directory.path)),
            Set(["projection.json", ".session-host.lock"])
        )
    }

    func testResumeMissingSaveFailsWithoutCreatingProjection() async throws {
        let directory = try makeSaveDirectory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let host = makeHost(directory)
        do {
            _ = try await host.command("resume_session")
            XCTFail("Expected missing save to be rejected")
        } catch {
            XCTAssertEqual((error as? HostCommandError)?.rawValue, HostCommandError.missingSave.rawValue)
        }
        XCTAssertFalse(FileManager.default.fileExists(atPath: directory.appendingPathComponent("projection.json").path))
    }

    func testResumeCorruptProjectionFailsWithoutResettingIt() async throws {
        let directory = try makeSaveDirectory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let projection = directory.appendingPathComponent("projection.json")
        let corrupt = Data("not a projection".utf8)
        try corrupt.write(to: projection)
        try Data().write(to: directory.appendingPathComponent("phase2.sqlite"))
        let host = makeHost(directory)
        do {
            _ = try await host.command("resume_session")
            XCTFail("Expected corrupt save to be rejected")
        } catch {
            XCTAssertEqual((error as? HostCommandError)?.rawValue, HostCommandError.openFailed.rawValue)
        }
        XCTAssertEqual(try Data(contentsOf: projection), corrupt)
    }

    func testSaveLockRejectsSecondHostUntilFirstCloses() async throws {
        let directory = try makeSaveDirectory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let first = makeHost(directory)
        let second = makeHost(directory)
        _ = try await first.command("new_game")
        do {
            _ = try await second.command("new_game", args: ["replaceExisting": true])
            XCTFail("Expected the second host to be locked out")
        } catch {
            XCTAssertEqual((error as? HostCommandError)?.rawValue, HostCommandError.saveLocked.rawValue)
        }
        _ = try await first.command("close_session")
        _ = try await second.command("resume_session")
        _ = try await second.command("close_session")
    }

    func testCommittedTranscriptSurvivesCloseAndResume() async throws {
        let directory = try makeSaveDirectory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let first = makeHost(directory)
        _ = try await first.command("new_game")
        _ = try await first.command("submit_input", args: ["text": "go to Connolly Cottage"])
        try await waitUntilSettled(first)
        _ = try await first.command("persist_session")
        let original = try await transcriptTexts(first)
        _ = try await first.command("close_session")

        let resumed = makeHost(directory)
        _ = try await resumed.command("resume_session")
        let resumedTranscript = try await transcriptTexts(resumed)
        XCTAssertEqual(resumedTranscript, original)
        _ = try await resumed.command("close_session")
    }

    func testPersistenceFailureIsReportedWhenSaveCannotBeReplaced() async throws {
        let directory = try makeSaveDirectory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let host = makeHost(directory)
        _ = try await host.command("new_game")
        _ = try await host.command("persist_session")
        let projection = directory.appendingPathComponent("projection.json")
        let savedProjection = try Data(contentsOf: projection)
        try FileManager.default.removeItem(at: projection)
        try FileManager.default.createDirectory(at: projection, withIntermediateDirectories: false)
        do {
            _ = try await host.command("persist_session")
            XCTFail("Expected persistence failure")
        } catch {
            XCTAssertEqual((error as? HostCommandError)?.rawValue, HostCommandError.persistence.rawValue)
        }
        try FileManager.default.removeItem(at: projection)
        try savedProjection.write(to: projection)
        do {
            _ = try await host.command("close_session")
            XCTFail("Expected the earlier persistence error to remain visible on close")
        } catch {
            XCTAssertEqual((error as? HostCommandError)?.rawValue, HostCommandError.persistence.rawValue)
        }

        let resumed = makeHost(directory)
        _ = try await resumed.command("resume_session")
        _ = try await resumed.command("close_session")
    }

    func testClarificationRejectsStaleQuestionAfterAnswer() async throws {
        let directory = try makeSaveDirectory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let host = makeHost(directory)
        _ = try await host.command("new_game")
        _ = try await host.command("submit_input", args: ["text": "go to Connolly Cottage"])
        try await waitUntilSettled(host)
        _ = try await host.command("submit_input", args: ["text": "ask Connolly about the household"])
        try await waitUntilSettled(host)
        let presentation = try await presentation(host)
        let clarification = try XCTUnwrap(presentation["clarification"] as? [String: Any])
        let questionID = try XCTUnwrap(clarification["questionID"] as? String)
        let options = try XCTUnwrap(clarification["options"] as? [[String: Any]])
        let choiceID = try XCTUnwrap(options.first?["choiceID"] as? String)
        _ = try await host.command("answer_clarification", args: ["questionID": questionID, "choiceID": choiceID])
        try await waitUntilSettled(host)
        do {
            _ = try await host.command("answer_clarification", args: ["questionID": questionID, "choiceID": choiceID])
            XCTFail("Expected stale clarification answer to be rejected")
        } catch {
            XCTAssertEqual((error as? HostCommandError)?.rawValue, HostCommandError.staleChoice.rawValue)
        }
    }

    func testStreamingHistoryStopAndRetryRecovery() async throws {
        let directory = try makeSaveDirectory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let host = makeHost(directory)
        _ = try await host.command("new_game")

        _ = try await host.command("submit_input", args: ["text": "go to Connolly Cottage"])
        try await waitUntilSettled(host)
        _ = try await host.command("submit_input", args: ["text": "Róisín, what do you trade?"])
        let streaming = try await waitForStreaming(host)
        XCTAssertTrue(streaming["isStreaming"] as? Bool ?? false)
        let framesWhileStreaming = try await history(host)
        XCTAssertTrue(framesWhileStreaming.contains { $0["isStreaming"] as? Bool == true })
        _ = try await host.command("stop")
        try await waitUntilSettled(host)
        let stopped = try await presentation(host)
        XCTAssertFalse(stopped["isStreaming"] as? Bool ?? true)

        _ = try await host.command("submit_input", args: ["text": "Róisín, offline once, what news?"])
        try await waitUntilSettled(host)
        let failed = try await presentation(host)
        XCTAssertNotNil(failed["failure"] as? String)
        _ = try await host.command("retry")
        _ = try await waitForEngineStreaming(host)
        try await waitUntilSettled(host)
        let recovered = try await presentation(host)
        XCTAssertFalse(recovered["isStreaming"] as? Bool ?? true)
        XCTAssertNil(recovered["failure"] as? String)
        XCTAssertEqual((recovered["latestRequest"] as? [String: String])?["phase"], "completed")
        _ = try await host.command("close_session")
    }

    private func makeHost(_ directory: URL) -> MachineHost {
        MachineHost(options: HostOptions(
            inference: .mock,
            modDirectory: Self.repositoryRoot.appendingPathComponent("mods/rundale"),
            saveDirectory: directory,
            turnTimeoutNanoseconds: 45_000_000_000
        ))
    }

    private func makeSaveDirectory() throws -> URL {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("machine-host-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory
    }

    private func response(_ host: MachineHost) async throws -> [String: Any] {
        try await host.command("session_presentation")
    }

    private func presentation(_ host: MachineHost) async throws -> [String: Any] {
        let value = try await response(host)
        return try XCTUnwrap(value["presentation"] as? [String: Any])
    }

    private func history(_ host: MachineHost) async throws -> [[String: Any]] {
        let value = try await response(host)
        return try XCTUnwrap(value["history"] as? [[String: Any]])
    }

    private func transcriptTexts(_ host: MachineHost) async throws -> [String] {
        let state = try await presentation(host)
        let transcript = try XCTUnwrap(state["transcript"] as? [[String: Any]])
        return transcript.compactMap { $0["text"] as? String }
    }

    private func waitForStreaming(_ host: MachineHost) async throws -> [String: Any] {
        let deadline = DispatchTime.now().uptimeNanoseconds + 20_000_000_000
        while DispatchTime.now().uptimeNanoseconds < deadline {
            let state = try await presentation(host)
            if state["isStreaming"] as? Bool == true { return state }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        XCTFail("The mock reply never entered streaming state")
        return try await presentation(host)
    }

    private func waitUntilSettled(_ host: MachineHost) async throws {
        let deadline = DispatchTime.now().uptimeNanoseconds + 45_000_000_000
        while DispatchTime.now().uptimeNanoseconds < deadline {
            let state = try await presentation(host)
            if state["isStreaming"] as? Bool == false, state["controlPending"] as? Bool != true { return }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        XCTFail("The mock reply did not settle before the deadline")
    }

    private func waitForEngineStreaming(_ host: MachineHost) async throws -> [String: Any] {
        let deadline = DispatchTime.now().uptimeNanoseconds + 20_000_000_000
        while DispatchTime.now().uptimeNanoseconds < deadline {
            let state = try await presentation(host)
            if state["isStreaming"] as? Bool == true, state["controlPending"] as? Bool != true {
                return state
            }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        XCTFail("The retry did not reach engine streaming state")
        return try await presentation(host)
    }
}
