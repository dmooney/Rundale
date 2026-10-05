import Foundation
import LimerickEndpointKit
import UIKit

/// One bug report on its way to `limerick-bug-report` (#2022), which keeps it
/// in a private inbox for triage. Kept on disk until the service has it, so a
/// report made offline is sent later.
struct PendingBugReport: Codable, Equatable, Sendable {
    let reportID: String
    let description: String
    let report: String
    let build: String?
    let device: String
    /// Base64 PNG.
    let screenshot: String?

    /// The service's wire names (bug-report/test/fixtures/phone-report.json).
    enum CodingKeys: String, CodingKey {
        case reportID = "reportId"
        case description, report, build, device, screenshot
    }
}

enum BugReportSendError: Error, Equatable {
    /// The phone has no usable connection; try again when it does.
    case offline
    /// Worth trying again later: a busy or failing service, or credentials
    /// that could not be had.
    case retryLater
    /// The service will never accept this report.
    case rejected
}

protocol BugReportTransport: Sendable {
    func send(_ report: PendingBugReport) async throws(BugReportSendError)
}

/// Holds unsent reports in Application Support and sends them in order.
@MainActor
final class BugReportOutbox {
    private let directory: URL
    private let transport: any BugReportTransport

    init(directory: URL, transport: any BugReportTransport) {
        self.directory = directory
        self.transport = transport
    }

    static func defaultDirectory(isUITesting: Bool) -> URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent(isUITesting ? "RundaleUITests/BugReports" : "Rundale/BugReports")
    }

    func enqueue(_ report: PendingBugReport) throws {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try JSONEncoder().encode(report).write(to: file(for: report.reportID), options: [.atomic])
    }

    var pending: [PendingBugReport] {
        let files = (try? FileManager.default.contentsOfDirectory(
            at: directory, includingPropertiesForKeys: [.creationDateKey]
        )) ?? []
        return files
            .filter { $0.pathExtension == "json" }
            .sorted { creation($0) < creation($1) }
            .compactMap { try? JSONDecoder().decode(PendingBugReport.self, from: Data(contentsOf: $0)) }
    }

    /// Sends one queued report. A delivered or rejected report leaves the
    /// queue; one that should be retried stays.
    func send(_ report: PendingBugReport) async throws(BugReportSendError) {
        do {
            try await transport.send(report)
            try? FileManager.default.removeItem(at: file(for: report.reportID))
        } catch .rejected {
            try? FileManager.default.removeItem(at: file(for: report.reportID))
            throw .rejected
        } catch {
            throw error
        }
    }

    private func file(for reportID: String) -> URL {
        directory.appendingPathComponent("\(reportID).json")
    }

    private func creation(_ url: URL) -> Date {
        (try? url.resourceValues(forKeys: [.creationDateKey]).creationDate) ?? .distantPast
    }
}

/// Posts reports to `<base>/v1/reports` with the same Firebase ID and App
/// Check tokens the app sends to Limerick Endpoints.
struct HTTPBugReportTransport: BugReportTransport {
    private static let offlineCodes: Set<URLError.Code> = [
        .notConnectedToInternet, .networkConnectionLost, .cannotFindHost, .cannotConnectToHost,
        .timedOut, .dataNotAllowed, .internationalRoamingOff,
    ]

    let baseURL: URL
    let credentials: any LimerickEndpointKit.EndpointCredentialProvider
    var session: URLSession = .shared

    func send(_ report: PendingBugReport) async throws(BugReportSendError) {
        do {
            let tokens = try await credentials.credentials()
            var request = URLRequest(url: baseURL.appendingPathComponent("v1/reports"))
            request.httpMethod = "POST"
            request.timeoutInterval = 60
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.setValue("Bearer \(tokens.authorizationToken)", forHTTPHeaderField: "Authorization")
            if let appCheck = tokens.appCheckToken {
                request.setValue(appCheck, forHTTPHeaderField: "X-Firebase-AppCheck")
            }
            request.httpBody = try JSONEncoder().encode(report)
            let (_, response) = try await session.data(for: request)
            let status = (response as? HTTPURLResponse)?.statusCode ?? 0
            if status == 202 { return }
            // 400 and 413 will never succeed; anything else may later.
            throw status == 400 || status == 413 ? BugReportSendError.rejected : .retryLater
        } catch let error as BugReportSendError {
            throw error
        } catch let error as URLError where Self.offlineCodes.contains(error.code) {
            throw .offline
        } catch {
            throw .retryLater
        }
    }
}

/// UI tests send into memory instead of the live service.
final class RecordingBugReportTransport: BugReportTransport, @unchecked Sendable {
    private let lock = NSLock()
    private var sent: [PendingBugReport] = []

    func send(_ report: PendingBugReport) async throws(BugReportSendError) {
        lock.withLock { sent.append(report) }
    }
}

enum ScreenCapture {
    /// A PNG of the key window as the player sees it, at 2x so it stays well
    /// under the service's 6 MB limit.
    @MainActor
    static func keyWindowPNG() -> Data? {
        let window = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap(\.windows)
            .first(where: \.isKeyWindow)
        guard let window else { return nil }
        let format = UIGraphicsImageRendererFormat()
        format.scale = 2
        return UIGraphicsImageRenderer(bounds: window.bounds, format: format).pngData { _ in
            window.drawHierarchy(in: window.bounds, afterScreenUpdates: false)
        }
    }

    /// The device model and OS, for the report.
    static var deviceDescription: String {
        var info = utsname()
        uname(&info)
        let model = withUnsafeBytes(of: &info.machine) { buffer in
            String(bytes: buffer.prefix { $0 != 0 }, encoding: .utf8)
        } ?? "iPhone"
        return "\(model) iOS \(ProcessInfo.processInfo.operatingSystemVersionString)"
    }
}
