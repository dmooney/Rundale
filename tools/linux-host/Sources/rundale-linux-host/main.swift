import Foundation
import RundaleLinuxHostCore

/// `rundale-linux-host --mod-dir DIR --save-dir DIR [--inference fixture|mock|sse]
/// [--sse-fixture FILE] [--resume] [--script FILE]`
///
/// Plays the app's game on Linux and prints each meaningful screen state as
/// a Markdown frame on standard output. Input lines come from `--script` or
/// standard input; `:help` lists the host commands.
@MainActor
func runHost() async -> Int32 {
    let arguments = Array(CommandLine.arguments.dropFirst())
    func value(_ name: String) -> String? {
        arguments.firstIndex(of: name).flatMap { arguments.indices.contains($0 + 1) ? arguments[$0 + 1] : nil }
    }
    func fail(_ message: String) -> Int32 {
        FileHandle.standardError.write(Data("rundale-linux-host: \(message)\n".utf8))
        return 2
    }
    let valued: Set = ["--mod-dir", "--save-dir", "--inference", "--sse-fixture", "--script"]
    var index = 0
    while index < arguments.count {
        let argument = arguments[index]
        if valued.contains(argument) {
            guard index + 1 < arguments.count else {
                return fail("\(argument) needs a value (see --help)")
            }
            index += 2
        } else if ["--resume", "--help"].contains(argument) {
            index += 1
        } else {
            return fail("unknown argument '\(argument)' (see --help)")
        }
    }
    if arguments.contains("--help") {
        print("""
        usage: rundale-linux-host --mod-dir DIR --save-dir DIR [--inference fixture|mock|sse]
                                  [--sse-fixture FILE] [--resume] [--script FILE]

        \(LinuxHost.help)
        """)
        return 0
    }
    guard let modDirectory = value("--mod-dir"), let saveDirectory = value("--save-dir") else {
        return fail("--mod-dir and --save-dir are required (see --help)")
    }
    let inference: HostInference
    switch value("--inference") ?? "mock" {
    case "fixture":
        inference = .fixture
    case "mock":
        inference = .mock
    case "sse":
        guard let fixture = value("--sse-fixture") else {
            return fail("--inference sse needs --sse-fixture FILE")
        }
        do {
            inference = try .sseFixture(SSEFixtureEndpointTransport(fixture: URL(fileURLWithPath: fixture)))
        } catch {
            return fail("cannot use --sse-fixture \(fixture): \(error)")
        }
    case let other:
        return fail("unknown inference mode '\(other)'; use fixture, mock, or sse")
    }

    let lines: AsyncStream<String>
    if let script = value("--script") {
        guard let text = try? String(contentsOfFile: script, encoding: .utf8) else {
            return fail("cannot read \(script)")
        }
        lines = AsyncStream { continuation in
            for line in text.components(separatedBy: .newlines) {
                continuation.yield(line)
            }
            continuation.finish()
        }
    } else {
        // Read standard input off the main actor, so a reply keeps streaming
        // while the host waits for the next line.
        lines = AsyncStream { continuation in
            Thread.detachNewThread {
                while let line = readLine() {
                    continuation.yield(line)
                }
                continuation.finish()
            }
        }
    }

    let host = LinuxHost(
        options: HostOptions(
            inference: inference,
            modDirectory: URL(fileURLWithPath: modDirectory, isDirectory: true),
            saveDirectory: URL(fileURLWithPath: saveDirectory, isDirectory: true),
            resume: arguments.contains("--resume")
        ),
        output: { print($0) }
    )
    await host.open()
    var quit = false
    for await line in lines {
        guard await host.perform(line) else {
            quit = true
            break
        }
    }
    if !quit {
        await host.close()
    }
    return host.timedOut ? 1 : 0
}

exit(await runHost())
