import Foundation
import UIKit

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
