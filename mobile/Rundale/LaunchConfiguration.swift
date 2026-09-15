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
        static let internalDiagnostics = "RUNDALE_INTERNAL_DIAGNOSTICS"
    }

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
    /// UI tests normally exercise the iPhone multiline composer. The explicit
    /// simulator keyboard case retains coverage of Mac Return-to-send behavior.
    let usesMultilineSimulatorComposer: Bool
    /// The embedded Limerick runtime is the normal product launch. Fixture
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
    /// A trusted Limerick Endpoints base URL supplied by deployment
    /// configuration. There is intentionally no baked-in production default.
    let endpointBaseURL: URL?
    let endpointOrganization: String
    let endpointSlug: String
    let endpointVersion: Int
    let internalDiagnosticsEnabled: Bool

    init(arguments: [String] = ProcessInfo.processInfo.arguments,
         environment: [String: String] = ProcessInfo.processInfo.environment,
         bundle: [String: Any] = Bundle.main.infoDictionary ?? [:]) {
        isUITesting = arguments.contains("--ui-tests")
        usesMultilineSimulatorComposer = isUITesting && !arguments.contains("--simulator-return-key")
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
        draftFileURL = arguments.first(where: { $0.hasPrefix("--draft-file=") })
            .map { URL(fileURLWithPath: String($0.dropFirst("--draft-file=".count))) }
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
                                                           bundle: bundle) ?? "2") ?? 2)
        internalDiagnosticsEnabled = Self.configuredValue(
            BundleKey.internalDiagnostics,
            environment: environment,
            bundle: bundle
        ).map { ["1", "true", "yes"].contains($0.lowercased()) } ?? false
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
