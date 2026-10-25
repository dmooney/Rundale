import Foundation
import LimerickEndpointKit
import OSLog
import RundaleSession

/// Firebase credentials for Limerick Endpoints and `limerick-bug-report`.
@MainActor
final class FirebaseEndpointCredentialAdapter: LimerickEndpointKit.EndpointCredentialProvider, @unchecked Sendable {
    private let provider: FirebaseEndpointCredentialProvider

    init(provider: FirebaseEndpointCredentialProvider) {
        self.provider = provider
    }

    nonisolated func credentials() async throws -> LimerickEndpointKit.EndpointCredentials {
        let credentials = try await provider.credentials()
        return LimerickEndpointKit.EndpointCredentials(
            authorizationToken: credentials.authorizationBearer,
            appCheckToken: credentials.appCheckToken
        )
    }
}

private let endpointLog = Logger(subsystem: "com.rundale.mobile", category: "endpoint")

extension RundaleEngineController {
    /// The app's engine session: Firebase credentials, the canonical mod
    /// bundled under `Mods/`, and Endpoint failures in the
    /// `com.rundale.mobile:endpoint` log.
    convenience init(configuration: LaunchConfiguration) {
        self.init(
            configuration: configuration,
            credentials: { FirebaseEndpointCredentialAdapter(provider: FirebaseEndpointCredentialProvider()) },
            modDirectory: Bundle.main.url(forResource: "rundale", withExtension: nil, subdirectory: "Mods"),
            log: { message in endpointLog.error("\(message, privacy: .public)") }
        )
    }
}
