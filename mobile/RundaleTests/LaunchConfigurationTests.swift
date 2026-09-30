import XCTest
@testable import Rundale

final class LaunchConfigurationTests: XCTestCase {
    func testBundleConfigurationProvidesReleaseEndpointIdentity() {
        let configuration = LaunchConfiguration(
            arguments: [],
            environment: [:],
            bundle: [
                "RUNDALE_ENDPOINT_BASE_URL": "https://example.test",
                "RUNDALE_ENDPOINT_ORGANIZATION": "limerick-demo"
            ]
        )

        XCTAssertEqual(configuration.endpointBaseURL?.absoluteString, "https://example.test")
        XCTAssertEqual(configuration.endpointOrganization, "limerick-demo")
        // Each model call names its own Endpoint; the app supplies the origin
        // and organization.
        XCTAssertEqual(configuration.endpointURL(slug: "rundale-dialogue", version: 1)?.absoluteString,
                       "https://example.test/v1/endpoints/limerick-demo/rundale-dialogue/versions/1/stream")
        XCTAssertEqual(configuration.endpointURL(slug: "rundale-intent", version: 1, stream: false)?.absoluteString,
                       "https://example.test/v1/endpoints/limerick-demo/rundale-intent/versions/1")
    }

    func testEnvironmentOverridesBundledEndpointConfiguration() {
        let configuration = LaunchConfiguration(
            arguments: [],
            environment: [
                "RUNDALE_ENDPOINT_BASE_URL": "http://localhost:8000",
                "RUNDALE_ENDPOINT_ORGANIZATION": "test-org"
            ],
            bundle: [
                "RUNDALE_ENDPOINT_BASE_URL": "https://example.test",
                "RUNDALE_ENDPOINT_ORGANIZATION": "limerick-demo"
            ]
        )

        XCTAssertEqual(configuration.endpointBaseURL?.absoluteString, "http://localhost:8000")
        XCTAssertEqual(configuration.endpointOrganization, "test-org")
    }

    func testMissingBundledEndpointRemainsUnconfigured() {
        let configuration = LaunchConfiguration(arguments: [], environment: [:], bundle: [:])

        XCTAssertNil(configuration.endpointBaseURL)
        XCTAssertNil(configuration.endpointURL(slug: "rundale-dialogue", version: 1))
    }

    func testUITestingUsesAnIsolatedApplicationSupportDraftByDefault() {
        let configuration = LaunchConfiguration(
            arguments: ["--ui-tests"],
            environment: [:],
            bundle: [:]
        )

        XCTAssertEqual(configuration.draftFileURL?.lastPathComponent, "phase1-draft.json")
        XCTAssertTrue(configuration.draftFileURL?.path.contains("RundaleUITests") == true)
    }

    func testOrdinaryLaunchLeavesDraftOverrideUnset() {
        let configuration = LaunchConfiguration(arguments: [], environment: [:], bundle: [:])

        XCTAssertNil(configuration.draftFileURL)
    }

    func testExplicitDraftOverrideWinsForUITesting() {
        let override = "/tmp/rundale-test/projection.json"
        let configuration = LaunchConfiguration(
            arguments: ["--ui-tests", "--draft-file=\(override)"],
            environment: [:],
            bundle: [:]
        )

        XCTAssertEqual(configuration.draftFileURL?.path, override)
    }

    func testExplicitMultilineFixtureDemoKeepsAutomaticStreaming() {
        let configuration = LaunchConfiguration(
            arguments: ["--fixture=standard", "--multiline-simulator-composer"],
            environment: [:],
            bundle: [:]
        )

        XCTAssertFalse(configuration.isUITesting)
        XCTAssertTrue(configuration.usesMultilineSimulatorComposer)
        XCTAssertFalse(configuration.manualStream)
    }

    func testPagedHistoryFixtureIsAnExplicitDeterministicLaunchMode() {
        let configuration = LaunchConfiguration(
            arguments: ["--ui-tests", "--fixture=paged-history"],
            environment: [:],
            bundle: [:]
        )

        XCTAssertEqual(configuration.fixture, .pagedHistory)
        XCTAssertTrue(configuration.isUITesting)
    }
}
