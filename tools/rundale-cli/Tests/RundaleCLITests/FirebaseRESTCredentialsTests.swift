import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import XCTest
@testable import RundaleCLICore

final class FirebaseRESTCredentialsTests: XCTestCase {
    private final class TestClock: @unchecked Sendable {
        private let lock = NSLock()
        private var value: Date

        init(_ value: Date) { self.value = value }

        var now: Date {
            lock.lock()
            defer { lock.unlock() }
            return value
        }

        func advance(by seconds: TimeInterval) {
            lock.lock()
            value = value.addingTimeInterval(seconds)
            lock.unlock()
        }
    }

    private final class Fixture: @unchecked Sendable {
        private let lock = NSLock()
        private var requests: [URLRequest] = []
        private var refreshCount = 0
        var delay: UInt64 = 0
        var failRefresh = false
        var malformedSignUp = false
        var malformedAppCheck = false

        func transport(_ request: URLRequest) async throws -> (Data, URLResponse) {
            let count = record(request)
            if delay > 0 { try await Task.sleep(nanoseconds: delay) }
            let url = request.url!.absoluteString
            let object: [String: Any]
            if url.contains("accounts:signUp") {
                object = malformedSignUp
                    ? ["localId": "user-1", "idToken": "", "refreshToken": "refresh-1", "expiresIn": "nonsense"]
                    : ["localId": "user-1", "idToken": "id-1", "refreshToken": "refresh-1", "expiresIn": "3600"]
            } else if url.contains("securetoken") {
                let current = incrementRefreshCount()
                if failRefresh { return try response(status: 400, object: ["error": "secret-detail", "token": "do-not-leak"]) }
                object = ["user_id": "user-1", "id_token": "id-\(current + 1)", "refresh_token": "refresh-\(current + 1)", "expires_in": "3600"]
            } else {
                object = malformedAppCheck ? ["token": "appcheck", "ttl": "unknown"] : ["token": "appcheck-\(count)", "ttl": "3600s"]
            }
            return try response(status: 200, object: object)
        }

        func allRequests() -> [URLRequest] {
            lock.lock()
            defer { lock.unlock() }
            return requests
        }

        private func record(_ request: URLRequest) -> Int {
            lock.lock()
            defer { lock.unlock() }
            requests.append(request)
            return requests.count
        }

        private func incrementRefreshCount() -> Int {
            lock.lock()
            defer { lock.unlock() }
            refreshCount += 1
            return refreshCount
        }

        private func response(status: Int, object: [String: Any]) throws -> (Data, URLResponse) {
            let data = try JSONSerialization.data(withJSONObject: object)
            guard let url = URL(string: "https://example.test"),
                  let response = HTTPURLResponse(url: url, statusCode: status, httpVersion: nil, headerFields: nil) else {
                throw FixtureError.invalidResponse
            }
            return (data, response)
        }
    }

    private enum FixtureError: Error {
        case invalidResponse
    }

    private func configuration() -> FirebaseRESTConfiguration {
        .init(apiKey: "test-key", appCheckDebugToken: "debug-token")
    }

    func testInitialAuthenticationAndAppCheckExchange() async throws {
        let fixture = Fixture()
        let provider = FirebaseRESTCredentialProvider(configuration: configuration(), transport: fixture.transport)
        let credentials = try await provider.credentials()
        XCTAssertEqual(credentials.authorizationToken, "id-1")
        XCTAssertNotNil(credentials.appCheckToken)
        let second = try await provider.credentials()
        XCTAssertEqual(second.appCheckToken, credentials.appCheckToken)
        let requests = fixture.allRequests()
        XCTAssertEqual(requests.count, 2)
        XCTAssertTrue(requests[0].url!.absoluteString.contains("accounts:signUp?key=test-key"))
        XCTAssertEqual(requests[0].value(forHTTPHeaderField: "X-Ios-Bundle-Identifier"), "com.rundale.mobile")
        XCTAssertTrue(requests[1].url!.absoluteString.contains("exchangeDebugToken"))
    }

    func testRefreshPreservesIdentityAndUsesRotatedRefreshToken() async throws {
        let fixture = Fixture()
        let clock = TestClock(Date(timeIntervalSince1970: 1_000))
        let provider = FirebaseRESTCredentialProvider(configuration: configuration(), transport: fixture.transport, clock: { clock.now })
        _ = try await provider.credentials()
        clock.advance(by: 3_550)
        let refreshed = try await provider.credentials()
        XCTAssertEqual(refreshed.authorizationToken, "id-2")
        let requests = fixture.allRequests()
        XCTAssertTrue(String(data: requests[2].httpBody!, encoding: .utf8)!.contains("refresh_token=refresh-1"))
        clock.advance(by: 3_550)
        _ = try await provider.credentials()
        let later = fixture.allRequests()[4]
        XCTAssertTrue(String(data: later.httpBody!, encoding: .utf8)!.contains("refresh_token=refresh-2"))
    }

    func testCredentialRequestsUseHTTPSAndEncodeAPIKey() async throws {
        let fixture = Fixture()
        let clock = TestClock(Date(timeIntervalSince1970: 1_000))
        let apiKey = "key&other=value+#"
        let config = FirebaseRESTConfiguration(apiKey: apiKey, appCheckDebugToken: "debug-token")
        let provider = FirebaseRESTCredentialProvider(configuration: config, transport: fixture.transport, clock: { clock.now })
        _ = try await provider.credentials()
        clock.advance(by: 3_550)
        _ = try await provider.credentials()

        let requests = fixture.allRequests()
        XCTAssertEqual(requests.count, 4)
        XCTAssertEqual(requests.map { $0.url?.host }, [
            "identitytoolkit.googleapis.com", "firebaseappcheck.googleapis.com",
            "securetoken.googleapis.com", "firebaseappcheck.googleapis.com"
        ])
        for request in requests {
            let url = try XCTUnwrap(request.url)
            XCTAssertEqual(url.scheme, "https")
            XCTAssertTrue(url.absoluteString.contains("%2B"))
            XCTAssertEqual(URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems, [
                URLQueryItem(name: "key", value: apiKey)
            ])
            XCTAssertEqual(request.value(forHTTPHeaderField: "X-Ios-Bundle-Identifier"), config.bundleID)
        }
        XCTAssertEqual(requests[1].url?.path, "/v1/projects/\(config.projectID)/apps/\(config.appID):exchangeDebugToken")
    }

    func testConcurrentCallsShareInitialIdentityCreation() async throws {
        let fixture = Fixture()
        fixture.delay = 20_000_000
        let provider = FirebaseRESTCredentialProvider(configuration: configuration(), transport: fixture.transport)
        async let first = provider.credentials()
        async let second = provider.credentials()
        let (firstCredentials, secondCredentials) = try await (first, second)
        XCTAssertEqual(firstCredentials.authorizationToken, secondCredentials.authorizationToken)
        XCTAssertEqual(fixture.allRequests().filter { $0.url!.absoluteString.contains("accounts:signUp") }.count, 1)
    }

    func testRefreshFailureDoesNotCreateNewIdentityAndIsSanitized() async throws {
        let fixture = Fixture()
        let clock = TestClock(Date(timeIntervalSince1970: 1_000))
        let provider = FirebaseRESTCredentialProvider(configuration: configuration(), transport: fixture.transport, clock: { clock.now })
        _ = try await provider.credentials()
        clock.advance(by: 3_550)
        fixture.failRefresh = true
        do {
            _ = try await provider.credentials()
            XCTFail("Expected refresh failure")
        } catch {
            XCTAssertEqual(error.localizedDescription, "Firebase token refresh failed.")
            XCTAssertFalse(error.localizedDescription.contains("secret-detail"))
            XCTAssertFalse(error.localizedDescription.contains("do-not-leak"))
        }
        XCTAssertEqual(fixture.allRequests().filter { $0.url!.absoluteString.contains("accounts:signUp") }.count, 1)
    }

    func testEnvironmentConfigurationDefaultsAndRequiredValues() {
        let config = FirebaseRESTConfiguration.fromEnvironment([
            "RUNDALE_FIREBASE_API_KEY": "key", "RUNDALE_APPCHECK_DEBUG_TOKEN": "debug"
        ])
        XCTAssertEqual(config?.projectID, "limerick-prod")
        XCTAssertEqual(config?.bundleID, "com.rundale.mobile")
        XCTAssertNil(FirebaseRESTConfiguration.fromEnvironment(["RUNDALE_FIREBASE_API_KEY": "key"]))
    }

    func testMalformedAuthenticationAndAppCheckResponsesFailClosed() async throws {
        let malformedAuth = Fixture()
        malformedAuth.malformedSignUp = true
        let authProvider = FirebaseRESTCredentialProvider(configuration: configuration(), transport: malformedAuth.transport)
        do {
            _ = try await authProvider.credentials()
            XCTFail("Expected malformed authentication response to fail")
        } catch {
            XCTAssertEqual(error.localizedDescription, "Firebase authentication failed.")
        }

        let malformedAppCheck = Fixture()
        malformedAppCheck.malformedAppCheck = true
        let appCheckProvider = FirebaseRESTCredentialProvider(configuration: configuration(), transport: malformedAppCheck.transport)
        do {
            _ = try await appCheckProvider.credentials()
            XCTFail("Expected malformed App Check response to fail")
        } catch {
            XCTAssertEqual(error.localizedDescription, "Firebase App Check exchange failed.")
        }
    }
}
