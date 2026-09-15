import XCTest
@testable import Rundale

final class LaunchConfigurationTests: XCTestCase {
    func testBundleConfigurationProvidesReleaseEndpointIdentity() {
        let configuration = LaunchConfiguration(
            arguments: [],
            environment: [:],
            bundle: [
                "RUNDALE_ENDPOINT_BASE_URL": "https://example.test",
                "RUNDALE_ENDPOINT_ORGANIZATION": "parish-demo",
                "RUNDALE_ENDPOINT_SLUG": "rundale-dialogue",
                "RUNDALE_ENDPOINT_VERSION": "7"
            ]
        )

        XCTAssertEqual(configuration.endpointBaseURL?.absoluteString, "https://example.test")
        XCTAssertEqual(configuration.endpointOrganization, "parish-demo")
        XCTAssertEqual(configuration.endpointSlug, "rundale-dialogue")
        XCTAssertEqual(configuration.endpointVersion, 7)
        XCTAssertEqual(configuration.endpointURL?.absoluteString,
                       "https://example.test/v1/endpoints/parish-demo/rundale-dialogue/versions/7/stream")
    }

    func testEnvironmentOverridesBundledEndpointConfiguration() {
        let configuration = LaunchConfiguration(
            arguments: [],
            environment: [
                "RUNDALE_ENDPOINT_BASE_URL": "http://localhost:8000",
                "RUNDALE_ENDPOINT_ORGANIZATION": "test-org",
                "RUNDALE_ENDPOINT_SLUG": "fixture-dialogue",
                "RUNDALE_ENDPOINT_VERSION": "2"
            ],
            bundle: [
                "RUNDALE_ENDPOINT_BASE_URL": "https://example.test",
                "RUNDALE_ENDPOINT_ORGANIZATION": "parish-demo",
                "RUNDALE_ENDPOINT_SLUG": "rundale-dialogue",
                "RUNDALE_ENDPOINT_VERSION": "7"
            ]
        )

        XCTAssertEqual(configuration.endpointBaseURL?.absoluteString, "http://localhost:8000")
        XCTAssertEqual(configuration.endpointOrganization, "test-org")
        XCTAssertEqual(configuration.endpointSlug, "fixture-dialogue")
        XCTAssertEqual(configuration.endpointVersion, 2)
    }

    func testMissingBundledEndpointRemainsUnconfigured() {
        let configuration = LaunchConfiguration(arguments: [], environment: [:], bundle: [:])

        XCTAssertNil(configuration.endpointBaseURL)
        XCTAssertNil(configuration.endpointURL)
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
}
