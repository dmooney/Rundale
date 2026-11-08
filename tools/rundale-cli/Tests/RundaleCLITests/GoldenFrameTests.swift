import Foundation
import RundaleCLICore
import XCTest

/// Plays each script in `Scenarios/` through the Rundale CLI and compares the
/// frames with `Goldens/`. The engine is the real one, linked from its native
/// static library, on the canonical mod; Endpoint answers come from the
/// scripted mock, a recorded stream, or the fixture session, as the
/// script's `# inference:` line says.
///
/// `RUNDALE_UPDATE_GOLDENS=1 swift test` rewrites the goldens; review the
/// diff before committing it.
@MainActor
final class GoldenFrameTests: XCTestCase {
    private static let packageDirectory = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
    private static let repositoryRoot = packageDirectory
        .deletingLastPathComponent()
        .deletingLastPathComponent()
    private static let testDirectory = URL(fileURLWithPath: #filePath).deletingLastPathComponent()

    func testNewGame() async throws {
        try await check("new-game")
    }

    func testDialogueStreaming() async throws {
        try await check("dialogue-streaming")
    }

    func testClarificationAnsweredThenSavedAndResumed() async throws {
        try await check("clarification-save-resume")
    }

    func testStopDuringStreaming() async throws {
        try await check("stop-during-streaming")
    }

    func testEndpointFailureThenRetry() async throws {
        try await check("endpoint-failure-retry")
    }

    func testRecordedDialogueStream() async throws {
        try await check("sse-fixture-dialogue")
    }

    func testFixtureSession() async throws {
        try await check("fixture-session")
    }

    func testExitStopsBeforeLaterLines() async throws {
        try await check("exit")
    }

    func testRejectsAFileThatIsNotARecordedStream() async throws {
        let file = FileManager.default.temporaryDirectory.appendingPathComponent("not-a-stream-\(UUID()).sse")
        try Data("event: text_delta\ndata: {\"type\":\"text_delta\",\"text\":\"hi\"}\n\n".utf8).write(to: file)
        defer { try? FileManager.default.removeItem(at: file) }
        XCTAssertThrowsError(try SSEFixtureEndpointTransport(fixture: file))
        XCTAssertThrowsError(try SSEFixtureEndpointTransport(
            fixture: Self.testDirectory.appendingPathComponent("missing.sse")
        ))
        await Task.yield()
    }

    func testEveryScenarioHasATest() async throws {
        let scenarios = try FileManager.default
            .contentsOfDirectory(atPath: Self.testDirectory.appendingPathComponent("Scenarios").path)
            .filter { $0.hasSuffix(".txt") }
            .map { String($0.dropLast(".txt".count)) }
        let tested: Set = [
            "new-game", "dialogue-streaming", "clarification-save-resume", "stop-during-streaming",
            "endpoint-failure-retry", "sse-fixture-dialogue", "fixture-session", "exit"
        ]
        XCTAssertEqual(Set(scenarios), tested)
        await Task.yield()
    }

    private func check(_ name: String) async throws {
        let script = try String(
            contentsOf: Self.testDirectory.appendingPathComponent("Scenarios/\(name).txt"), encoding: .utf8
        )
        let lines = script.components(separatedBy: "\n")
        let saveDirectory = FileManager.default.temporaryDirectory
            .appendingPathComponent("rundale-cli-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: saveDirectory) }

        var frames = ""
        let host = CLIHost(
            options: HostOptions(
                inference: try inference(lines),
                modDirectory: Self.repositoryRoot.appendingPathComponent("mods/rundale"),
                saveDirectory: saveDirectory
            ),
            output: { frames += $0 + "\n" }
        )
        await host.open()
        for line in lines {
            guard await host.perform(line) else { break }
        }
        await host.close()
        XCTAssertFalse(host.timedOut, "a turn timed out:\n\(frames)")

        // Golden frames use a platform-neutral label.
        // Runtime evidence still retains the actual platform in all output.
        frames = frames.replacingOccurrences(of: HostPlatform.label, with: "Rundale CLI")
        let golden = Self.testDirectory.appendingPathComponent("Goldens/\(name).md")
        if ProcessInfo.processInfo.environment["RUNDALE_UPDATE_GOLDENS"] == "1" {
            try frames.write(to: golden, atomically: true, encoding: .utf8)
            return
        }
        let expected = try String(contentsOf: golden, encoding: .utf8)
        if frames != expected {
            // CI uploads this directory when the job fails.
            let directory = Self.packageDirectory.appendingPathComponent(".build/golden-actual")
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            let actual = directory.appendingPathComponent("\(name).md")
            try frames.write(to: actual, atomically: true, encoding: .utf8)
            XCTFail("""
            Frames differ from Goldens/\(name).md; they are in \(actual.path). \
            If the change is intended, rerun with RUNDALE_UPDATE_GOLDENS=1 and review the diff.
            """)
        }
    }

    private func inference(_ lines: [String]) throws -> HostInference {
        let header = lines.first { $0.hasPrefix("# inference: ") }.map { String($0.dropFirst("# inference: ".count)) }
        switch header {
        case "fixture":
            return .fixture
        case "mock":
            return .mock
        case "sse":
            return try .sseFixture(SSEFixtureEndpointTransport(
                fixture: Self.repositoryRoot.appendingPathComponent("mobile/endpoint/fixtures/dialogue-v1.sse")
            ))
        default:
            throw UnknownInference(header: header)
        }
    }
}

private struct UnknownInference: Error {
    let header: String?
}
