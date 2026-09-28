import XCTest
@testable import Rundale

/// The FFI boundary answers every session request with `not_wired` until
/// #2044 connects it to the shared engine. These tests run against the real
/// linked Rust library, not a stub.
@MainActor
final class EngineNotWiredTests: XCTestCase {
    private var directory: URL!

    override func setUp() async throws {
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("EngineNotWiredTests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: directory)
    }

    func testNormalLaunchShowsNotWiredMessage() async throws {
        let launch = LaunchConfiguration(
            arguments: [
                "--ui-tests",
                "--phase2",
                "--draft-file=\(directory.appendingPathComponent("projection.json").path)"
            ],
            environment: [:],
            bundle: [:]
        )
        XCTAssertTrue(launch.phase2, "the engine controller is selected")
        let model = RundalePresentationModel(launch: launch)
        model.start()

        for _ in 0..<500 where model.submissionMessage == nil {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(model.submissionMessage, RundaleEngineController.notWiredMessage)
        XCTAssertTrue(model.transcript.isEmpty, "no game runs, so nothing is committed")
    }
}
