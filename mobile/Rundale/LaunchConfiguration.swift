import Foundation

/// Process arguments are intentionally the only fixture controls exposed by
/// the app. They make UI automation deterministic without adding a product
/// debug surface to the running experience.
struct LaunchConfiguration: Sendable {
    private enum BundleKey {
        static let endpointBaseURL = "RUNDALE_ENDPOINT_BASE_URL"
        static let endpointOrganization = "RUNDALE_ENDPOINT_ORGANIZATION"
        static let endpointSlug = "RUNDALE_ENDPOINT_SLUG"
        static let endpointVersion = "RUNDALE_ENDPOINT_VERSION"
        static let intentEndpointSlug = "RUNDALE_INTENT_ENDPOINT_SLUG"
        static let intentEndpointVersion = "RUNDALE_INTENT_ENDPOINT_VERSION"
    }

    enum Fixture: String, Equatable, Sendable {
        case standard
        case manualStream = "manual-stream"
        case longHistory = "long-history"
        /// A >500-row fixture used only to prove native transcript paging.
        /// Keep `long-history` compact for the ordinary Phase 1 walkthrough.
        case pagedHistory = "paged-history"
        case failed
        case rejected
        case interrupted
        case clarification
        case restoration
        case restorationLongHistory = "restoration-long-history"
    }

    let isUITesting: Bool
    /// UI tests normally exercise the iPhone multiline composer. An explicit
    /// fixture-demo argument can select it while retaining automatic stepping.
    /// The simulator keyboard case retains coverage of Mac Return-to-send.
    let usesMultilineSimulatorComposer: Bool
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
    /// The Intent Endpoint is a separate published contract with its own
    /// output schema, so it is deployed and versioned independently of the
    /// dialogue Endpoint. Role selection stays in Rust; this is transport.
    let intentEndpointSlug: String
    let intentEndpointVersion: Int

    init(arguments: [String] = ProcessInfo.processInfo.arguments,
         environment: [String: String] = ProcessInfo.processInfo.environment,
         bundle: [String: Any] = Bundle.main.infoDictionary ?? [:]) {
        isUITesting = arguments.contains("--ui-tests")
        usesMultilineSimulatorComposer = !arguments.contains("--simulator-return-key")
            && (isUITesting || arguments.contains("--multiline-simulator-composer"))
        let hasExplicitFixture = arguments.contains { $0.hasPrefix("--fixture=") }
        phase2 = arguments.contains("--phase2")
            || arguments.contains("--phase3")
            || (!isUITesting && !hasExplicitFixture)
        phase2MockTransport = phase2
            && isUITesting
            && (arguments.contains("--phase2-mock") || arguments.contains("--phase3-mock"))
        let requestedFixture = arguments.first(where: { $0.hasPrefix("--fixture=") })
            .flatMap { Fixture(rawValue: String($0.dropFirst("--fixture=".count))) }
            ?? .standard

        fixture = requestedFixture
        manualStream = !phase2 && (isUITesting || arguments.contains("--manual-stream"))
        autoFocusComposer = !arguments.contains("--no-auto-focus")
        initialDraft = environment["RUNDALE_UI_TEST_DRAFT"]
        // UI automation must never open the player's ordinary local save. The
        // explicit override remains authoritative for isolated test fixtures
        // and for the engine's sibling SQLite path.
        draftFileURL = arguments.first(where: { $0.hasPrefix("--draft-file=") })
            .map { URL(fileURLWithPath: String($0.dropFirst("--draft-file=".count))) }
            ?? (isUITesting
                ? FileManager.default
                    .urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
                    .appendingPathComponent(phase2 ? "RundaleUITests/engine/phase2-projection.json"
                        : "RundaleUITests/fixture/phase1-draft.json")
                : nil)
        resetFixture = arguments.contains("--reset-fixture")
        forceDarkAppearance = isUITesting && arguments.contains("--force-dark-appearance")

        let configuredBaseURL = Self.configuredValue(BundleKey.endpointBaseURL,
                                                      environment: environment,
                                                      bundle: bundle)
        endpointBaseURL = configuredBaseURL.flatMap(URL.init(string:))
        endpointOrganization = Self.configuredValue(BundleKey.endpointOrganization,
                                                     environment: environment,
                                                     bundle: bundle) ?? "rundale"
        endpointSlug = Self.configuredValue(BundleKey.endpointSlug,
                                             environment: environment,
                                             bundle: bundle) ?? "rundale-dialogue"
        endpointVersion = max(1, Int(Self.configuredValue(BundleKey.endpointVersion,
                                                           environment: environment,
                                                           bundle: bundle) ?? "1") ?? 1)
        intentEndpointSlug = Self.configuredValue(BundleKey.intentEndpointSlug,
                                                   environment: environment,
                                                   bundle: bundle) ?? "rundale-intent"
        intentEndpointVersion = max(1, Int(Self.configuredValue(BundleKey.intentEndpointVersion,
                                                                 environment: environment,
                                                                 bundle: bundle) ?? "1") ?? 1)
    }

    private static func configuredValue(_ key: String,
                                       environment: [String: String],
                                       bundle: [String: Any]) -> String? {
        if let value = environment[key]?.trimmingCharacters(in: .whitespacesAndNewlines),
           !value.isEmpty {
            return value
        }
        if let value = bundle[key] as? String {
            let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
            return trimmed.isEmpty ? nil : trimmed
        }
        return nil
    }

    var endpointURL: URL? {
        endpointURL(slug: endpointSlug, version: endpointVersion)
    }

    /// The deployed Endpoint for one engine-selected role.
    ///
    /// The engine owns role selection; the app only knows which deployment
    /// serves each published contract.
    func endpointURL(role: String) -> URL? {
        role == "intent"
            ? endpointURL(slug: intentEndpointSlug, version: intentEndpointVersion)
            : endpointURL(slug: endpointSlug, version: endpointVersion)
    }

    func endpointVersion(role: String) -> Int {
        role == "intent" ? intentEndpointVersion : endpointVersion
    }

    private func endpointURL(slug: String, version: Int) -> URL? {
        guard let endpointBaseURL else { return nil }
        return endpointBaseURL
            .appendingPathComponent("v1")
            .appendingPathComponent("endpoints")
            .appendingPathComponent(endpointOrganization)
            .appendingPathComponent(slug)
            .appendingPathComponent("versions")
            .appendingPathComponent(String(version))
            .appendingPathComponent("stream")
    }
}
