import Foundation
import Swiftdansi
#if os(Linux)
import Glibc
#else
import Darwin
#endif

enum TerminalOutputFormat: String {
    case auto, ansi, markdown
}

/// Presentation belongs at the executable boundary: protocol frames stay Markdown.
struct TerminalOutput {
    private let rendersMarkdown: Bool
    private let usesColor: Bool

    init(format: TerminalOutputFormat) {
        let environment = ProcessInfo.processInfo.environment
        rendersMarkdown = format == .ansi || (
            format == .auto && isatty(STDOUT_FILENO) != 0 && environment["TERM"] != "dumb"
        )
        usesColor = format == .ansi || (environment["NO_COLOR"] ?? "").isEmpty
    }

    func render(_ frame: String) -> String {
        guard rendersMarkdown else { return frame }
        // Resolve width on every frame so terminal resizing takes effect.
        return Swiftdansi.render(frame, options: RenderOptions(hyperlinks: false, color: usesColor))
    }
}
