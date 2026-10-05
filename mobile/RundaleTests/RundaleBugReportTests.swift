import UIKit
import XCTest
import RundaleKit
@testable import Rundale

/// `/bug` and shake-to-report send a report to `limerick-bug-report` in beta
/// builds, never as a turn, and a report made offline waits on disk (#2022,
/// docs/plans/mobile-bug-report.md).
@MainActor
final class RundaleBugReportTests: XCTestCase {
    private var directory: URL!
    private var transport: ScriptedTransport!

    override func setUp() async throws {
        try await super.setUp()
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("RundaleBugReportTests-\(UUID().uuidString)")
        transport = ScriptedTransport()
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: directory)
        try await super.tearDown()
    }

    private func outbox() -> BugReportOutbox {
        BugReportOutbox(directory: directory, transport: transport)
    }

    private func model(beta: Bool, session: Phase4TestSession) -> RundalePresentationModel {
        RundalePresentationModel(
            launch: LaunchConfiguration(
                arguments: ["--fixture=standard"],
                environment: [:],
                bundle: beta ? ["RUNDALE_BETA_FEEDBACK": "YES",
                                "CFBundleShortVersionString": "0.1.0",
                                "CFBundleVersion": "1284"] : [:]
            ),
            session: session,
            bugReports: outbox(),
            captureScreenshot: { Data([0x89, 0x50, 0x4E, 0x47]) }
        )
    }

    func testBugCommandSendsTheReportAndClearsTheDraftWithoutSubmitting() async {
        let session = Phase4TestSession(active: false)
        let model = model(beta: true, session: session)
        model.draft = "/bug  the miller ignored me "
        model.noteDraftMutation()

        model.submitDraft()
        XCTAssertEqual(model.bugReportNotice, RundalePresentationModel.bugReportSendingNotice)
        await waitUntil { model.bugReportNotice == RundalePresentationModel.bugReportSentNotice }

        XCTAssertEqual(session.bugReportDescriptions, ["the miller ignored me"])
        XCTAssertEqual(session.submitCalls, 0, "/bug never becomes a turn")
        let sent = transport.sent
        XCTAssertEqual(sent.count, 1)
        XCTAssertEqual(sent.first?.description, "the miller ignored me")
        XCTAssertEqual(sent.first?.report, "Rundale bug report\nthe miller ignored me\n")
        XCTAssertEqual(sent.first?.build, "0.1.0 (1284)")
        XCTAssertEqual(sent.first?.screenshot, Data([0x89, 0x50, 0x4E, 0x47]).base64EncodedString())
        XCTAssertEqual(model.draft, "")
        XCTAssertTrue(outbox().pending.isEmpty, "a sent report leaves the queue")
    }

    func testBugCommandWorksWhileAReplyStreams() async {
        let session = Phase4TestSession(active: true)
        let model = model(beta: true, session: session)
        XCTAssertTrue(model.isStreaming)
        model.draft = "/BUG"
        model.submitDraft()
        await waitUntil { self.transport.sent.count == 1 }
        XCTAssertEqual(session.bugReportDescriptions, [""])
        XCTAssertEqual(session.submitCalls, 0)
    }

    func testShakeSendsTheReportAndKeepsTheDraft() async {
        let session = Phase4TestSession(active: false)
        let model = model(beta: true, session: session)
        model.draft = "half a thought"
        model.noteDraftMutation()

        model.reportBug()
        await waitUntil { self.transport.sent.count == 1 }

        XCTAssertEqual(session.bugReportDescriptions, [""])
        XCTAssertEqual(model.draft, "half a thought")
        model.dismissBugReportNotice()
        XCTAssertNil(model.bugReportNotice)
    }

    func testAReportMadeOfflineWaitsAndIsSentAtTheNextLaunch() async {
        transport.next = [.failure(.retryLater)]
        let first = model(beta: true, session: Phase4TestSession(active: false))
        first.reportBug(description: "no signal in the bog")
        await waitUntil { first.bugReportNotice == RundalePresentationModel.bugReportQueuedNotice }
        XCTAssertEqual(outbox().pending.map(\.description), ["no signal in the bog"])

        let relaunched = model(beta: true, session: Phase4TestSession(active: false))
        relaunched.start()
        await waitUntil {
            relaunched.bugReportNotice == RundalePresentationModel.earlierBugReportSentNotice
        }
        XCTAssertTrue(outbox().pending.isEmpty)
        XCTAssertEqual(transport.sent.map(\.description), ["no signal in the bog", "no signal in the bog"])
        XCTAssertEqual(Set(transport.sent.map(\.reportID)).count, 1, "a resend keeps its identity")
    }

    func testARejectedReportLeavesTheQueue() async {
        transport.next = [.failure(.rejected)]
        let model = model(beta: true, session: Phase4TestSession(active: false))
        model.reportBug()
        await waitUntil { model.bugReportNotice == RundalePresentationModel.bugReportRejectedNotice }
        XCTAssertTrue(outbox().pending.isEmpty)
    }

    func testDraftEditedWhileTheReportIsMadeIsKept() async {
        let model = model(beta: true, session: Phase4TestSession(active: false))
        model.draft = "/bug"
        model.noteDraftMutation()
        model.submitDraft()
        model.draft = "go to the cottage"
        model.noteDraftMutation()
        await waitUntil { self.transport.sent.count == 1 }
        XCTAssertEqual(model.draft, "go to the cottage")
    }

    func testOutsideBetaBuildsBugIsAnOrdinaryCommandAndShakeDoesNothing() async {
        let session = Phase4TestSession(active: false)
        let model = model(beta: false, session: session)
        model.reportBug()
        model.draft = "/bug"
        model.noteDraftMutation()
        model.submitDraft()
        await waitUntil { session.submitCalls == 1 }
        XCTAssertEqual(session.bugReportDescriptions, [])
        XCTAssertTrue(transport.sent.isEmpty)
        XCTAssertNil(model.bugReportNotice)
    }

    func testABetaBuildWithoutTheServiceSaysSo() {
        let model = RundalePresentationModel(
            launch: LaunchConfiguration(arguments: ["--fixture=standard"], environment: [:],
                                        bundle: ["RUNDALE_BETA_FEEDBACK": "YES"]),
            session: Phase4TestSession(active: false)
        )
        model.reportBug()
        XCTAssertEqual(model.bugReportNotice, RundalePresentationModel.bugReportUnavailableNotice)
    }

    /// The phone and limerick-bug-report agree on the wire: both sides test
    /// against bug-report/test/fixtures/phone-report.json.
    func testTheReportEncodesAsTheServiceExpects() throws {
        let fixtureURL = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .appendingPathComponent("../../bug-report/test/fixtures/phone-report.json")
            .standardizedFileURL
        let fixture = try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(contentsOf: fixtureURL)) as? [String: String]
        )
        let report = PendingBugReport(
            reportID: try XCTUnwrap(fixture["reportId"]),
            description: try XCTUnwrap(fixture["description"]),
            report: try XCTUnwrap(fixture["report"]),
            build: fixture["build"],
            device: try XCTUnwrap(fixture["device"]),
            screenshot: fixture["screenshot"]
        )
        let encoded = try XCTUnwrap(
            JSONSerialization.jsonObject(with: JSONEncoder().encode(report)) as? [String: String]
        )
        XCTAssertEqual(encoded, fixture)
    }

    func testOnlyTheBugWordIsTheCommand() {
        XCTAssertEqual(RundalePresentationModel.bugDescription(in: "/bug"), "")
        XCTAssertEqual(RundalePresentationModel.bugDescription(in: " /bug\nMícheál said nothing "),
                       "Mícheál said nothing")
        XCTAssertNil(RundalePresentationModel.bugDescription(in: "/bugle"))
        XCTAssertNil(RundalePresentationModel.bugDescription(in: "ask about the /bug"))
    }

    func testBetaFlagServiceAndBuildComeFromTheBundle() {
        let beta = LaunchConfiguration(arguments: [], environment: [:], bundle: [
            "RUNDALE_BETA_FEEDBACK": "YES",
            "RUNDALE_BUG_REPORT_URL": "https://bugs.example.test",
            "CFBundleShortVersionString": "0.1.0",
            "CFBundleVersion": "2180"
        ])
        XCTAssertTrue(beta.allowsBugReports)
        XCTAssertEqual(beta.bugReportURL?.absoluteString, "https://bugs.example.test")
        XCTAssertEqual(beta.buildDescription, "0.1.0 (2180)")
        let store = LaunchConfiguration(arguments: [], environment: [:], bundle: [
            "RUNDALE_BETA_FEEDBACK": "NO"
        ])
        XCTAssertFalse(store.allowsBugReports)
        XCTAssertNil(store.bugReportURL)
        XCTAssertNil(store.buildDescription)
    }

    /// A shake reaches the window whatever has focus; the window announces
    /// it, and nothing else.
    func testWindowAnnouncesAShakeAndIgnoresOtherMotion() {
        let window = UIWindow(frame: .zero)
        let shaken = expectation(forNotification: .rundaleDeviceDidShake, object: window)
        window.motionEnded(.motionShake, with: nil)
        wait(for: [shaken], timeout: 1)

        let other = expectation(forNotification: .rundaleDeviceDidShake, object: window)
        other.isInverted = true
        window.motionEnded(.remoteControlPlay, with: nil)
        wait(for: [other], timeout: 0.2)
    }

    private func waitUntil(
        _ condition: @escaping @MainActor () -> Bool,
        file: StaticString = #filePath,
        line: UInt = #line
    ) async {
        for _ in 0..<500 {
            if condition() { return }
            await Task.yield()
        }
        XCTFail("condition did not become true", file: file, line: line)
    }
}

/// Answers each send from `next`, then accepts.
private final class ScriptedTransport: BugReportTransport, @unchecked Sendable {
    private let lock = NSLock()
    private var _sent: [PendingBugReport] = []
    var next: [Result<Void, BugReportSendError>] = []

    var sent: [PendingBugReport] { lock.withLock { _sent } }

    func send(_ report: PendingBugReport) async throws(BugReportSendError) {
        let outcome: Result<Void, BugReportSendError> = lock.withLock {
            _sent.append(report)
            if !next.isEmpty, case let .failure(error) = next.removeFirst() {
                return .failure(error)
            }
            return .success(())
        }
        try outcome.get()
    }
}
