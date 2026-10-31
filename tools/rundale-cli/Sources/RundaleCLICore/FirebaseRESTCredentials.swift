import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import LimerickEndpointKit

public struct FirebaseRESTConfiguration: Sendable {
    public let apiKey: String
    public let appCheckDebugToken: String
    public let appID: String
    public let projectID: String
    public let bundleID: String

    public init(
        apiKey: String,
        appCheckDebugToken: String,
        appID: String = "1:877612517009:ios:586f98a2cc3e7d0c676130",
        projectID: String = "limerick-prod",
        bundleID: String = "com.rundale.mobile"
    ) {
        self.apiKey = apiKey
        self.appCheckDebugToken = appCheckDebugToken
        self.appID = appID
        self.projectID = projectID
        self.bundleID = bundleID
    }

    public static func fromEnvironment(_ environment: [String: String] = ProcessInfo.processInfo.environment) -> Self? {
        guard let apiKey = environment["RUNDALE_FIREBASE_API_KEY"], !apiKey.isEmpty,
              let debugToken = environment["RUNDALE_APPCHECK_DEBUG_TOKEN"], !debugToken.isEmpty else { return nil }
        return Self(
            apiKey: apiKey,
            appCheckDebugToken: debugToken,
            appID: environment["RUNDALE_FIREBASE_APP_ID"] ?? "1:877612517009:ios:586f98a2cc3e7d0c676130",
            projectID: environment["RUNDALE_FIREBASE_PROJECT_ID"] ?? "limerick-prod",
            bundleID: environment["RUNDALE_FIREBASE_BUNDLE_ID"] ?? "com.rundale.mobile"
        )
    }
}

public actor FirebaseRESTCredentialProvider: EndpointCredentialProvider {
    public typealias HTTPTransport = @Sendable (URLRequest) async throws -> (Data, URLResponse)
    public typealias Clock = @Sendable () -> Date

    private struct Identity: Sendable {
        let localID: String
        let idToken: String
        let refreshToken: String
        let expiry: Date
    }

    private struct AppCheckCredential: Sendable {
        let token: String
        let expiry: Date
    }

    private let configuration: FirebaseRESTConfiguration
    private let transport: HTTPTransport
    private let clock: Clock
    private var identity: Identity?
    private var appCheckCredential: AppCheckCredential?
    private var inFlight: Task<EndpointCredentials, Error>?

    public init(
        configuration: FirebaseRESTConfiguration,
        transport: HTTPTransport? = nil,
        clock: @escaping Clock = Date.init
    ) {
        self.configuration = configuration
        self.transport = transport ?? Self.urlSessionTransport
        self.clock = clock
    }

    public func credentials() async throws -> EndpointCredentials {
        if let inFlight { return try await inFlight.value }
        let task = Task { try await self.loadCredentials() }
        inFlight = task
        defer { inFlight = nil }
        return try await task.value
    }

    private func loadCredentials() async throws -> EndpointCredentials {
        let auth: Identity
        if let existing = identity {
            if existing.expiry.timeIntervalSince(clock()) > 60 {
                auth = existing
            } else {
                auth = try await refresh(existing)
                identity = auth
            }
        } else {
            auth = try await signUp()
            identity = auth
        }
        let appCheck = try await appCheckToken()
        return EndpointCredentials(authorizationToken: auth.idToken, appCheckToken: appCheck)
    }

    private func signUp() async throws -> Identity {
        let data = try await sendJSON(
            url: urlWithAPIKey(URL(string: "https://identitytoolkit.googleapis.com/v1/accounts:signUp")!),
            method: "POST",
            body: ["returnSecureToken": true]
        )
        guard let localID = nonemptyString(data["localId"]),
              let idToken = nonemptyString(data["idToken"]),
              let refreshToken = nonemptyString(data["refreshToken"]),
              let seconds = expirySeconds(data["expiresIn"]) else {
            throw FirebaseRESTCredentialError.authentication
        }
        return Identity(localID: localID, idToken: idToken, refreshToken: refreshToken, expiry: clock().addingTimeInterval(seconds))
    }

    private func refresh(_ previous: Identity) async throws -> Identity {
        var request = URLRequest(url: urlWithAPIKey(URL(string: "https://securetoken.googleapis.com/v1/token")!))
        request.httpMethod = "POST"
        request.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
        request.setValue(configuration.bundleID, forHTTPHeaderField: "X-Ios-Bundle-Identifier")
        var form = URLComponents()
        form.queryItems = [URLQueryItem(name: "grant_type", value: "refresh_token"), URLQueryItem(name: "refresh_token", value: previous.refreshToken)]
        request.httpBody = form.percentEncodedQuery?.data(using: .utf8)
        let data: [String: Any]
        do {
            data = try await send(request)
        } catch {
            throw FirebaseRESTCredentialError.refresh
        }
        guard let idToken = nonemptyString(data["id_token"]),
              let refreshToken = nonemptyString(data["refresh_token"]),
              let userID = nonemptyString(data["user_id"]),
              userID == previous.localID,
              let seconds = expirySeconds(data["expires_in"]) else {
            throw FirebaseRESTCredentialError.refresh
        }
        return Identity(localID: previous.localID, idToken: idToken, refreshToken: refreshToken, expiry: clock().addingTimeInterval(seconds))
    }

    private func appCheckToken() async throws -> String {
        if let appCheckCredential, appCheckCredential.expiry.timeIntervalSince(clock()) > 60 {
            return appCheckCredential.token
        }
        let data: [String: Any]
        do {
            data = try await sendJSON(
                url: urlWithAPIKey(
                    URL(string: "https://firebaseappcheck.googleapis.com/v1/projects")!
                        .appendingPathComponent(configuration.projectID)
                        .appendingPathComponent("apps")
                        .appendingPathComponent("\(configuration.appID):exchangeDebugToken")
                ),
                method: "POST",
                body: ["debugToken": configuration.appCheckDebugToken]
            )
        } catch {
            throw FirebaseRESTCredentialError.appCheck
        }
        guard let token = nonemptyString(data["token"]),
              let ttl = durationSeconds(data["ttl"]) else { throw FirebaseRESTCredentialError.appCheck }
        appCheckCredential = AppCheckCredential(token: token, expiry: clock().addingTimeInterval(ttl))
        return token
    }

    private func nonemptyString(_ value: Any?) -> String? {
        guard let string = value as? String, !string.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
        return string
    }

    private func expirySeconds(_ value: Any?) -> TimeInterval? {
        guard let string = nonemptyString(value), let seconds = TimeInterval(string), seconds.isFinite, seconds > 0 else { return nil }
        return seconds
    }

    private func durationSeconds(_ value: Any?) -> TimeInterval? {
        guard let string = nonemptyString(value), string.hasSuffix("s"),
              let seconds = TimeInterval(string.dropLast()), seconds.isFinite, seconds > 0 else { return nil }
        return seconds
    }

    private func urlWithAPIKey(_ url: URL) -> URL {
        var components = URLComponents(url: url, resolvingAgainstBaseURL: false)!
        components.queryItems = [URLQueryItem(name: "key", value: configuration.apiKey)]
        // Some servers decode a literal plus in a query as a space.
        components.percentEncodedQuery = components.percentEncodedQuery?.replacingOccurrences(of: "+", with: "%2B")
        return components.url!
    }

    private func sendJSON(url: URL, method: String, body: [String: Any]) async throws -> [String: Any] {
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue(configuration.bundleID, forHTTPHeaderField: "X-Ios-Bundle-Identifier")
        request.httpBody = try JSONSerialization.data(withJSONObject: body)
        return try await send(request)
    }

    private func send(_ request: URLRequest) async throws -> [String: Any] {
        do {
            let (body, response) = try await transport(request)
            guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode),
                  let object = try JSONSerialization.jsonObject(with: body) as? [String: Any] else {
                throw FirebaseRESTCredentialError.authentication
            }
            return object
        } catch let error as FirebaseRESTCredentialError {
            throw error
        } catch {
            throw FirebaseRESTCredentialError.transport
        }
    }

    private static func urlSessionTransport(_ request: URLRequest) async throws -> (Data, URLResponse) {
        try await URLSession.shared.data(for: request)
    }
}

public enum FirebaseRESTCredentialError: Error, LocalizedError, Equatable {
    case authentication
    case refresh
    case appCheck
    case transport

    public var errorDescription: String? {
        switch self {
        case .authentication: "Firebase authentication failed."
        case .refresh: "Firebase token refresh failed."
        case .appCheck: "Firebase App Check exchange failed."
        case .transport: "Firebase credential request failed."
        }
    }
}
