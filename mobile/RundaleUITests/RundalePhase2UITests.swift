import XCTest

/// End-to-end checks for the embedded Limerick runtime.  These tests deliberately
/// launch the real application target and use only the deterministic Endpoint
/// transport that is compiled behind the Phase 2 UI-test arguments.
@MainActor
class RundalePhase2UITestCase: RundaleUITestCase {
    var app: XCUIApplication!

    override func setUp() async throws {
        try await super.setUp()
        continueAfterFailure = false
        app = XCUIApplication()
    }

    override func tearDown() async throws {
        app?.terminate()
        app = nil
        try await super.tearDown()
    }

    /// The opening scene on the canonical world (`mods/rundale`).
    static let opening = "A muddy road runs between low stone walls"

    func launch(reset: Bool, simulatorReturnKey: Bool = false) {
        app.launchArguments = [
            "--ui-tests",
            "--phase2",
            "--phase2-mock",
            "--no-auto-focus"
        ]
        if reset {
            app.launchArguments.append("--reset-fixture")
        }
        if simulatorReturnKey {
            app.launchArguments.append("--simulator-return-key")
        }
        app.launch()
    }

    func waitForInitialScene() {
        XCTAssertTrue(
            waitForTranscriptText(Self.opening, timeout: 8),
            "The embedded Rust runtime should publish its opening scene"
        )
        XCTAssertTrue(
            app.otherElements["status.header"].waitForExistence(timeout: 3),
            "The Phase 2 header should be present"
        )
    }

    func submit(_ command: String) {
        let input = commandInput
        XCTAssertTrue(input.waitForExistence(timeout: 5))
        input.tap()
        input.typeText(command)
        app.buttons["composer.send"].tap()
        // Acceptance clears the draft. The command row itself can scroll out
        // of the lazy transcript under a long scene, so it is not the signal.
        XCTAssertTrue(waitForValue("", on: input, timeout: 8), command)
    }

    var commandInput: XCUIElement {
        app.descendants(matching: .any)
            .matching(identifier: "composer.input")
            .firstMatch
    }

    func waitForTranscriptText(_ text: String, timeout: TimeInterval) -> Bool {
        app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@", text)
        ).firstMatch.waitForExistence(timeout: timeout)
    }

    func waitForTranscriptItem(_ text: String, timeout: TimeInterval) -> Bool {
        app.descendants(matching: .any).matching(
            NSPredicate(
                format: "identifier BEGINSWITH 'transcript.item.' AND label CONTAINS %@",
                text
            )
        ).firstMatch.waitForExistence(timeout: timeout)
    }

    func waitForValue(_ value: String,
                      on element: XCUIElement,
                      timeout: TimeInterval) -> Bool {
        let predicate = value.isEmpty
            ? NSPredicate(format: "value == '' OR value == nil")
            : NSPredicate(format: "value == %@", value)
        let expectation = XCTNSPredicateExpectation(predicate: predicate, object: element)
        return XCTWaiter.wait(for: [expectation], timeout: timeout) == .completed
    }
}

/// The engine boots the canonical world, and the simulator's Return key
/// sends. The engine's scenarios are tested in `EngineScenarioTests`.
@MainActor
final class RundalePhase2UITests: RundalePhase2UITestCase {
    func testPhase2FoundationBootsCanonicalWorldAndPeig() {
        launch(reset: true)

        let header = app.otherElements["status.header"]
        XCTAssertTrue(header.waitForExistence(timeout: 8))
        XCTAssertTrue(header.label.localizedCaseInsensitiveContains("Kilteevan Village"))
        XCTAssertTrue(waitForTranscriptText(Self.opening, timeout: 8))

        submit("/look")
        XCTAssertTrue(waitForTranscriptItem("It is morning.", timeout: 8))
        // Peig is walking up from the Letter Office at 07:00; she waits on
        // the village road for the morning post.
        submit("/wait 10")
        submit("/people")
        XCTAssertTrue(waitForTranscriptItem("a satchel of letters", timeout: 8))
    }

    func testSimulatorReturnKeySubmitsDraft() throws {
        #if !targetEnvironment(simulator)
        throw XCTSkip("The simulator return-key contract does not apply to a physical iPhone")
        #else
        launch(reset: true, simulatorReturnKey: true)
        waitForInitialScene()

        let input = commandInput
        input.tap()
        input.typeText("/look\n")

        // The scene /look prints can push the command row out of the lazy
        // transcript on a small screen, so read it from the trace.
        XCTAssertTrue(app.waitForTranscriptRow(timeout: 8) {
            $0.kind == "player_command" && $0.text.contains("/look")
        })
        XCTAssertTrue(waitForValue("", on: input, timeout: 8))
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 3))
        #endif
    }
}
