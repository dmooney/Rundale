import Foundation

/// Private protocol, distinct from MCP. One request/response per line;
/// action receipts never wait for a whole inference turn.
@MainActor
public enum HostProtocol {
    public static func serve(options: HostOptions) async -> Int32 {
        let host = MachineHost(options: options)
        for await line in inputLines() {
            var identifier: Any = NSNull()
            let response: [String: Any]
            var shutdown = false
            do {
                guard let request = try JSONSerialization.jsonObject(with: line) as? [String: Any],
                      request["version"] as? Int == 1,
                      let requestID = request["id"] as? UInt64,
                      let command = request["command"] as? String,
                      let args = request["args"] as? [String: Any] else {
                    throw HostCommandError.invalidArguments
                }
                identifier = requestID
                let result = try await host.command(command, args: args)
                response = ["version": 1, "id": requestID, "result": result]
                shutdown = command == "shutdown"
            } catch {
                let code = (error as? HostCommandError)?.rawValue ?? "hostFailure"
                response = ["version": 1, "id": identifier, "error": ["code": code, "message": "Session host: \(code)."]]
            }
            guard let data = try? JSONSerialization.data(withJSONObject: response, options: [.sortedKeys]),
                  data.count < 4 * 1024 * 1024 else {
                _ = try? await host.command("shutdown")
                return 1
            }
            FileHandle.standardOutput.write(data + Data([10]))
            if shutdown { return 0 }
        }
        _ = try? await host.command("shutdown")
        return 0
    }

    private static func inputLines() -> AsyncStream<Data> {
        AsyncStream { continuation in
            Thread.detachNewThread {
                var buffer = Data()
                while true {
                    let chunk = FileHandle.standardInput.availableData
                    if chunk.isEmpty { break }
                    buffer.append(chunk)
                    while let newline = buffer.firstIndex(of: 10) {
                        let line = buffer.prefix(upTo: newline)
                        guard line.count <= 1_048_576 else {
                            continuation.finish()
                            return
                        }
                        continuation.yield(Data(line))
                        buffer.removeSubrange(...newline)
                    }
                    guard buffer.count <= 1_048_576 else {
                        continuation.finish()
                        return
                    }
                }
                if !buffer.isEmpty { continuation.yield(buffer) }
                continuation.finish()
            }
        }
    }
}
