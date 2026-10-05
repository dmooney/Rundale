import UIKit
import XCTest
import RundaleKit
@testable import Rundale

/// `/bug` and shake-to-report copy a report for TestFlight feedback in beta
/// builds, never as a turn (#2022, docs/plans/mobile-bug-report.md).
@MainActor
final class RundaleBugReportTests: XCTestCase {
    private var copied: [String] = []

    private func model(beta: Bool, session: Phase4TestSession) -> RundalePresentationModel {
        RundalePresentationModel(
            launch: LaunchConfiguration(
                arguments: ["--fixture=standard"],
                environment: [:],
                bundle: beta ? ["RUNDALE_BETA_FEEDBACK": "YES"] : [:]
            ),
            session: session,
            copyToPasteboard: { [weak self] in self?.copied.append($0) }
        )
    }

    func testBugCommandCopiesTheReportAndClearsTheDraftWithoutSubmitting() async {
        let session = Phase4TestSession(active: false)
        let model = model(beta: true, session: session)
        model.draft = "/bug  the miller ignored me "
        model.noteDraftMutation()

        model.submitDraft()
        await waitUntil { model.bugReportNotice != nil }

        XCTAssertEqual(session.bugReportDescriptions, ["the miller ignored me"])
        XCTAssertEqual(session.submitCalls, 0, "/bug never becomes a turn")
        XCTAssertEqual(copied, ["Rundale bug report\nthe miller ignored me\n"])
        XCTAssertEqual(model.bugReportNotice, RundalePresentationModel.bugReportCopiedNotice)
        XCTAssertEqual(model.draft, "")
    }

    func testBugCommandWorksWhileAReplyStreams() async {
        let session = Phase4TestSession(active: true)
        let model = model(beta: true, session: session)
        model.start()
        XCTAssertTrue(model.isStreaming)
        model.draft = "/BUG"
        model.submitDraft()
        await waitUntil { !self.copied.isEmpty }
        XCTAssertEqual(session.bugReportDescriptions, [""])
        XCTAssertEqual(session.submitCalls, 0)
    }

    func testShakeCopiesTheReportAndKeepsTheDraft() async {
        let session = Phase4TestSession(active: false)
        let model = model(beta: true, session: session)
        model.draft = "half a thought"
        model.noteDraftMutation()

        model.reportBug()
        await waitUntil { !self.copied.isEmpty }

        XCTAssertEqual(session.bugReportDescriptions, [""])
        XCTAssertEqual(model.draft, "half a thought")
        model.dismissBugReportNotice()
        XCTAssertNil(model.bugReportNotice)
    }

    func testDraftEditedWhileTheReportIsMadeIsKept() async {
        let session = Phase4TestSession(active: false)
        let model = model(beta: true, session: session)
        model.draft = "/bug"
        model.noteDraftMutation()
        model.submitDraft()
        model.draft = "go to the cottage"
        model.noteDraftMutation()
        await waitUntil { !self.copied.isEmpty }
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
        XCTAssertEqual(copied, [])
        XCTAssertNil(model.bugReportNotice)
    }

    func testOnlyTheBugWordIsTheCommand() {
        XCTAssertEqual(RundalePresentationModel.bugDescription(in: "/bug"), "")
        XCTAssertEqual(RundalePresentationModel.bugDescription(in: " /bug\nMícheál said nothing "),
                       "Mícheál said nothing")
        XCTAssertNil(RundalePresentationModel.bugDescription(in: "/bugle"))
        XCTAssertNil(RundalePresentationModel.bugDescription(in: "ask about the /bug"))
    }

    func testBetaFlagAndBuildComeFromTheBundle() {
        let beta = LaunchConfiguration(arguments: [], environment: [:], bundle: [
            "RUNDALE_BETA_FEEDBACK": "YES",
            "CFBundleShortVersionString": "0.1.0",
            "CFBundleVersion": "2180"
        ])
        XCTAssertTrue(beta.allowsBugReports)
        XCTAssertEqual(beta.buildDescription, "0.1.0 (2180)")
        let store = LaunchConfiguration(arguments: [], environment: [:], bundle: [
            "RUNDALE_BETA_FEEDBACK": "NO"
        ])
        XCTAssertFalse(store.allowsBugReports)
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
        for _ in 0..<200 {
            if condition() { return }
            await Task.yield()
        }
        XCTFail("condition did not become true", file: file, line: line)
    }
}

