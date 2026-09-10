import Foundation

/// Process arguments are intentionally the only fixture controls exposed by
/// the app. They make UI automation deterministic without adding a product
/// debug surface to the running experience.
struct LaunchConfiguration: Sendable {
    enum Fixture: String, Equatable, Sendable {
        case standard
        case manualStream = "manual-stream"
        case longHistory = "long-history"
        case failed
        case rejected
        case interrupted
        case clarification
        case restoration
        case restorationLongHistory = "restoration-long-history"
    }

    let isUITesting: Bool
    /// The embedded Parish runtime is the normal product launch. Fixture
    /// launches remain available for the deterministic Phase 1 UI suite and
    /// for explicit fixture invocations.
    let phase2: Bool
    let phase2MockTransport: Bool
    let fixture: Fixture
    let manualStream: Bool
    let autoFocusComposer: Bool
    let initialDraft: String?
    let draftFileURL: URL?
    let resetFixture: Bool
    let forceDarkAppearance: Bool
    /// A trusted Parish Endpoints base URL supplied by deployment
    /// configuration. There is intentionally no baked-in production default.
    let endpointBaseURL: URL?
    let endpointOrganization: String
    let endpointSlug: String
    let endpointVersion: Int

    init(arguments: [String] = ProcessInfo.processInfo.arguments,
         environment: [String: String] = ProcessInfo.processInfo.environment) {
        isUITesting = arguments.contains("--ui-tests")
        let hasExplicitFixture = arguments.contains { $0.hasPrefix("--fixture=") }
        phase2 = arguments.contains("--phase2") || (!isUITesting && !hasExplicitFixture)
        phase2MockTransport = phase2 && isUITesting && arguments.contains("--phase2-mock")
        let requestedFixture = arguments.first(where: { $0.hasPrefix("--fixture=") })
            .flatMap { Fixture(rawValue: String($0.dropFirst("--fixture=".count))) }
            ?? .standard

        fixture = requestedFixture
        manualStream = !phase2 && (isUITesting || arguments.contains("--manual-stream"))
        autoFocusComposer = !arguments.contains("--no-auto-focus")
        initialDraft = environment["RUNDALE_UI_TEST_DRAFT"]
        draftFileURL = arguments.first(where: { $0.hasPrefix("--draft-file=") })
            .map { URL(fileURLWithPath: String($0.dropFirst("--draft-file=".count))) }
        resetFixture = arguments.contains("--reset-fixture")
        forceDarkAppearance = isUITesting && arguments.contains("--force-dark-appearance")

        endpointBaseURL = environment["RUNDALE_ENDPOINT_BASE_URL"].flatMap(URL.init(string:))
        endpointOrganization = environment["RUNDALE_ENDPOINT_ORGANIZATION"] ?? "rundale"
        endpointSlug = environment["RUNDALE_ENDPOINT_SLUG"] ?? "rundale-dialogue"
        endpointVersion = max(1, Int(environment["RUNDALE_ENDPOINT_VERSION"] ?? "1") ?? 1)
    }

    var endpointURL: URL? {
        guard let endpointBaseURL else { return nil }
        return endpointBaseURL
            .appendingPathComponent("v1")
            .appendingPathComponent("endpoints")
            .appendingPathComponent(endpointOrganization)
            .appendingPathComponent(endpointSlug)
            .appendingPathComponent("versions")
            .appendingPathComponent(String(endpointVersion))
            .appendingPathComponent("stream")
    }
}
