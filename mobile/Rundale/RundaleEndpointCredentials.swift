import Foundation

#if canImport(FirebaseAppCheck) && canImport(FirebaseAuth) && canImport(FirebaseCore)
import FirebaseAppCheck
import FirebaseAuth
import FirebaseCore
#endif

/// The credentials sent to a Parish Endpoint request.
///
/// `authorizationBearer` is a Firebase Auth ID token for the configured
/// Firebase project. It is not a model-provider token and is never persisted
/// by this boundary. `appCheckToken` proves that the request came from an
/// attested (or explicitly debug-authorized simulator) app instance.
public struct EndpointCredentials: Equatable, Sendable {
    public let authorizationBearer: String
    public let appCheckToken: String

    public init(authorizationBearer: String, appCheckToken: String) {
        self.authorizationBearer = authorizationBearer
        self.appCheckToken = appCheckToken
    }

    public var headers: [String: String] {
        [
            "Authorization": "Bearer \(authorizationBearer)",
            "X-Firebase-AppCheck": appCheckToken
        ]
    }
}

/// The credential boundary used by the mobile Endpoint transport.
///
/// The main-actor isolation keeps Firebase's Objective-C SDK objects on the
/// thread where Firebase configures them while leaving transports free to
/// inject a fake in tests.
@MainActor
public protocol EndpointCredentialProviding {
    func credentials(forceRefresh: Bool) async throws -> EndpointCredentials
}

@MainActor
public extension EndpointCredentialProviding {
    func credentials() async throws -> EndpointCredentials {
        try await credentials(forceRefresh: false)
    }

    /// Compatibility spelling for callers that prefer the full operation
    /// name. The protocol's single requirement remains `credentials` so test
    /// doubles stay small.
    func endpointCredentials(forceRefresh: Bool = false) async throws -> EndpointCredentials {
        try await credentials(forceRefresh: forceRefresh)
    }
}

/// A deterministic, actor-isolated fake for Endpoint transport tests.
@MainActor
public final class FakeEndpointCredentialProvider: EndpointCredentialProviding {
    public enum Failure: Error, Equatable, Sendable, LocalizedError {
        case unconfigured

        public var errorDescription: String? {
            "The fake Endpoint credential provider has no configured result."
        }
    }

    public var result: Result<EndpointCredentials, Failure>
    public private(set) var requestedRefreshes: [Bool] = []

    public init(credentials: EndpointCredentials) {
        result = .success(credentials)
    }

    public init(failure: Failure = .unconfigured) {
        result = .failure(failure)
    }

    public init(result: Result<EndpointCredentials, Failure>) {
        self.result = result
    }

    public func credentials(forceRefresh: Bool) async throws -> EndpointCredentials {
        requestedRefreshes.append(forceRefresh)
        return try result.get()
    }
}

/// Public, non-secret Firebase values that identify the app configuration the
/// Endpoint backend must accept.
public struct FirebaseEndpointCredentialConfiguration: Equatable, Sendable {
    public let googleServiceInfoURL: URL?
    public let expectedGoogleAppID: String?
    public let expectedProjectID: String?
    public let expectedBundleID: String?

    public init(
        googleServiceInfoURL: URL? = nil,
        expectedGoogleAppID: String? = "1:24861210203:ios:2df6bf4ed8c4828253b17e",
        expectedProjectID: String? = "cottage-d6dc9",
        expectedBundleID: String? = "com.rundale.mobile"
    ) {
        self.googleServiceInfoURL = googleServiceInfoURL
        self.expectedGoogleAppID = expectedGoogleAppID
        self.expectedProjectID = expectedProjectID
        self.expectedBundleID = expectedBundleID
    }
}

public enum EndpointCredentialError: Error, Equatable, Sendable, LocalizedError {
    case firebaseSDKUnavailable
    case missingFirebaseConfiguration
    case invalidFirebaseConfiguration
    case unexpectedFirebaseConfiguration
    case authenticatedUserUnavailable
    case emptyAuthorizationToken
    case emptyAppCheckToken

    public var errorDescription: String? {
        switch self {
        case .firebaseSDKUnavailable:
            return "Firebase support is unavailable in this build."
        case .missingFirebaseConfiguration:
            return "Firebase is not configured for this app build."
        case .invalidFirebaseConfiguration:
            return "Firebase configuration is invalid for this app build."
        case .unexpectedFirebaseConfiguration:
            return "Firebase configuration does not belong to the Rundale Endpoint app."
        case .authenticatedUserUnavailable:
            return "Firebase did not return an authenticated user."
        case .emptyAuthorizationToken:
            return "Firebase did not return an authorization token."
        case .emptyAppCheckToken:
            return "Firebase did not return an App Check token."
        }
    }
}

/// Lazily configures Firebase Auth and App Check when a remote Endpoint
/// request actually needs credentials.
///
/// Creating this provider, launching the app, and using local gameplay do not
/// configure Firebase or trigger authentication. Firebase configuration is
/// performed once on the main actor immediately before the first credential
/// request.
@MainActor
public final class FirebaseEndpointCredentialProvider: EndpointCredentialProviding {
    public let configuration: FirebaseEndpointCredentialConfiguration

    private var firebaseConfigured = false

    #if canImport(FirebaseAppCheck) && canImport(FirebaseAuth) && canImport(FirebaseCore)
    private var anonymousSignInTask: Task<Void, Error>?
    #endif

    public init(
        configuration: FirebaseEndpointCredentialConfiguration = .init()
    ) {
        self.configuration = configuration
    }

    public func credentials(forceRefresh: Bool = false) async throws -> EndpointCredentials {
        #if canImport(FirebaseAppCheck) && canImport(FirebaseAuth) && canImport(FirebaseCore)
        try configureFirebaseIfNeeded()

        let user = try await ensureAuthenticatedUser()
        // Keep Firebase's non-Sendable User on the main actor. Its callback
        // API returns only the owned token across the suspension boundary.
        let idToken: String = try await withCheckedThrowingContinuation { continuation in
            user.getIDTokenForcingRefresh(forceRefresh) { token, error in
                if let error {
                    continuation.resume(throwing: error)
                } else if let token {
                    continuation.resume(returning: token)
                } else {
                    continuation.resume(throwing: EndpointCredentialError.emptyAuthorizationToken)
                }
            }
        }
        guard !idToken.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw EndpointCredentialError.emptyAuthorizationToken
        }

        let appCheckToken = try await AppCheck.appCheck().token(forcingRefresh: forceRefresh).token
        guard !appCheckToken.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw EndpointCredentialError.emptyAppCheckToken
        }

        return EndpointCredentials(
            authorizationBearer: idToken,
            appCheckToken: appCheckToken
        )
        #else
        throw EndpointCredentialError.firebaseSDKUnavailable
        #endif
    }

    #if canImport(FirebaseAppCheck) && canImport(FirebaseAuth) && canImport(FirebaseCore)
    private func configureFirebaseIfNeeded() throws {
        if firebaseConfigured {
            return
        }

        if let app = FirebaseApp.app() {
            try validate(options: app.options)
            firebaseConfigured = true
            return
        }

        guard let url = configuration.googleServiceInfoURL
                ?? Bundle.main.url(forResource: "GoogleService-Info", withExtension: "plist") else {
            throw EndpointCredentialError.missingFirebaseConfiguration
        }
        guard let options = FirebaseOptions(contentsOfFile: url.path) else {
            throw EndpointCredentialError.invalidFirebaseConfiguration
        }
        try validate(options: options)

        // This must happen before FirebaseApp.configure. The simulator
        // provider is compiled only for private DEBUG simulator builds; all
        // other builds use real device attestation with an OS fallback.
        AppCheck.setAppCheckProviderFactory(RundaleAppCheckProviderFactory())
        FirebaseApp.configure(options: options)
        firebaseConfigured = true
    }

    private func validate(options: FirebaseOptions) throws {
        guard !options.googleAppID.isEmpty,
              !options.bundleID.isEmpty,
              let apiKey = options.apiKey,
              !apiKey.isEmpty,
              let projectID = options.projectID,
              !projectID.isEmpty else {
            throw EndpointCredentialError.invalidFirebaseConfiguration
        }

        if let expectedGoogleAppID = configuration.expectedGoogleAppID,
           options.googleAppID != expectedGoogleAppID {
            throw EndpointCredentialError.unexpectedFirebaseConfiguration
        }
        if let expectedProjectID = configuration.expectedProjectID,
           projectID != expectedProjectID {
            throw EndpointCredentialError.unexpectedFirebaseConfiguration
        }
        if let expectedBundleID = configuration.expectedBundleID,
           options.bundleID != expectedBundleID {
            throw EndpointCredentialError.unexpectedFirebaseConfiguration
        }
    }

    private func ensureAuthenticatedUser() async throws -> User {
        let auth = Auth.auth()
        if let currentUser = auth.currentUser {
            return currentUser
        }

        if let anonymousSignInTask {
            try await anonymousSignInTask.value
        } else {
            let signInTask = Task { @MainActor in
                _ = try await auth.signInAnonymously()
            }
            anonymousSignInTask = signInTask
            defer { anonymousSignInTask = nil }
            try await signInTask.value
        }

        guard let currentUser = auth.currentUser else {
            throw EndpointCredentialError.authenticatedUserUnavailable
        }
        return currentUser
    }
    #endif
}

#if canImport(FirebaseAppCheck) && canImport(FirebaseCore)
private final class RundaleAppCheckProviderFactory: NSObject, AppCheckProviderFactory {
    func createProvider(with app: FirebaseApp) -> AppCheckProvider? {
        #if DEBUG && targetEnvironment(simulator)
        // Firebase reads the optional AppCheckDebugToken environment variable
        // (or deprecated FIRAAppCheckDebugToken spelling) itself. No token is
        // copied into source, logs, errors, or persistent app state here.
        return AppCheckDebugProvider(app: app)
        #else
        if #available(iOS 14.0, *) {
            return AppAttestProvider(app: app)
        }
        return DeviceCheckProvider(app: app)
        #endif
    }
}
#endif
