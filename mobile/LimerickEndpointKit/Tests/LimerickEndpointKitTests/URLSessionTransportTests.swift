import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
#if canImport(Darwin)
import Darwin
#elseif canImport(Glibc)
import Glibc
#endif
import XCTest
@testable import LimerickEndpointKit

/// Drives the production `URLSessionEndpointTransport` against a loopback
/// HTTP server, so the real socket path is tested on macOS and on Linux
/// (swift-corelibs-foundation) rather than only through mock transports.
final class URLSessionTransportTests: XCTestCase {
    /// Generous upper bound for any one step. The tests wait on events, not
    /// on elapsed time, so a slow machine only makes them take longer.
    private static let stepTimeout: TimeInterval = 10

    func testEachSSEEventReachesTheClientBeforeTheServerSendsTheNext() async throws {
        let server = try LoopbackHTTPServer()
        defer { server.close() }
        let frames = FrameRecorder()
        let client = Self.client()
        let request = try Self.streamRequest(port: server.port)
        let consumer = Task {
            for try await frame in client.stream(request) { frames.append(frame) }
        }

        let received = try await server.acceptRequest(timeout: Self.stepTimeout)
        XCTAssertTrue(received.head.hasPrefix("POST /stream HTTP/1.1\r\n"), received.head)
        XCTAssertEqual(received.header("accept"), "text/event-stream")
        XCTAssertEqual(received.header("x-request-id"), "r")
        XCTAssertEqual(received.header("x-attempt-id"), "a")
        XCTAssertEqual(received.body, Data(#"{"input":{}}"#.utf8))

        try server.send(Self.sseResponseHead)
        let events = [
            Self.event(type: "progress", sequence: 1, payload: #""text":"Peig listens.""#),
            Self.event(type: "text_delta", sequence: 2, payload: #""text":"The rain ""#),
            Self.event(type: "final", sequence: 3, payload: #""output":{"dialogue":"The rain eased."}"#)
        ]
        for (index, event) in events.enumerated() {
            try server.sendChunk(event)
            // The server holds the next event until this one is delivered, so
            // a transport that buffered the body would time out here.
            let delivered = await frames.wait(forCount: index + 1, timeout: Self.stepTimeout)
            XCTAssertTrue(delivered, "event \(index + 1) was not delivered while the response stayed open")
            guard delivered else {
                consumer.cancel()
                return
            }
        }
        try server.send(Data("0\r\n\r\n".utf8))
        try await consumer.value

        XCTAssertEqual(frames.kinds(), [.progress, .textDelta, .final])
        XCTAssertEqual(frames.texts(), ["Peig listens.", "The rain ", nil])
    }

    func testCancellingTheStreamClosesTheConnection() async throws {
        let server = try LoopbackHTTPServer()
        defer { server.close() }
        let frames = FrameRecorder()
        let client = Self.client()
        let request = try Self.streamRequest(port: server.port)
        let consumer = Task {
            for try await frame in client.stream(request) { frames.append(frame) }
        }

        _ = try await server.acceptRequest(timeout: Self.stepTimeout)
        try server.send(Self.sseResponseHead)
        try server.sendChunk(Self.event(type: "progress", sequence: 1, payload: #""text":"Peig listens.""#))
        let delivered = await frames.wait(forCount: 1, timeout: Self.stepTimeout)
        XCTAssertTrue(delivered, "the first event was not delivered")

        // The response stays open with no terminal event; only the client's
        // cancel can end it.
        consumer.cancel()
        let closed = await server.waitForPeerClose(timeout: Self.stepTimeout)
        XCTAssertTrue(closed, "cancelling the stream must close the HTTP connection")
        _ = await consumer.result
        XCTAssertEqual(frames.kinds(), [.progress])
    }

    private static let sseResponseHead = Data((
        "HTTP/1.1 200 OK\r\n"
            + "Content-Type: text/event-stream\r\n"
            + "Cache-Control: no-cache\r\n"
            + "Transfer-Encoding: chunked\r\n"
            + "\r\n"
    ).utf8)

    private static func client() -> LimerickEndpointClient {
        LimerickEndpointClient(
            credentials: StaticEndpointCredentialProvider(.init(authorizationToken: "token")),
            transport: URLSessionEndpointTransport(session: URLSession(configuration: .ephemeral)),
            policy: EndpointURLPolicy(allowLoopbackHTTP: true)
        )
    }

    private static func streamRequest(port: UInt16) throws -> EndpointRequest {
        try EndpointRequest(
            url: URL(string: "http://127.0.0.1:\(port)/stream")!,
            requestID: "r",
            attemptID: "a",
            invocationID: "i",
            policy: EndpointURLPolicy(allowLoopbackHTTP: true),
            body: Data(#"{"input":{}}"#.utf8)
        )
    }

    private static func event(type: String, sequence: Int, payload: String) -> String {
        let data = #"{"contract_version":1,"request_id":"r","attempt_id":"a","invocation_id":"i","#
            + #""event_id":"i:\#(sequence)","sequence":\#(sequence),"type":"\#(type)","endpoint_version":1,"#
            + #""terminal":\#(type == "final"),\#(payload)}"#
        return "event: \(type)\nid: i:\(sequence)\ndata: \(data)\n\n"
    }
}

/// Frames the consumer task has received, read by the test as it paces the server.
private final class FrameRecorder: @unchecked Sendable {
    private let lock = NSLock()
    private var frames: [EndpointStreamFrame] = []

    func append(_ frame: EndpointStreamFrame) {
        lock.lock(); defer { lock.unlock() }
        frames.append(frame)
    }

    func kinds() -> [EndpointStreamFrame.Kind] {
        lock.lock(); defer { lock.unlock() }
        return frames.map(\.kind)
    }

    func texts() -> [String?] {
        lock.lock(); defer { lock.unlock() }
        return frames.map(\.text)
    }

    func wait(forCount count: Int, timeout: TimeInterval) async -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if frameCount() >= count { return true }
            try? await Task.sleep(nanoseconds: 5_000_000)
        }
        return frameCount() >= count
    }

    private func frameCount() -> Int {
        lock.lock(); defer { lock.unlock() }
        return frames.count
    }
}

private struct LoopbackRequest {
    let head: String
    let body: Data

    func header(_ name: String) -> String? {
        for line in head.components(separatedBy: "\r\n").dropFirst() {
            guard let colon = line.firstIndex(of: ":") else { continue }
            if line[..<colon].lowercased() == name {
                return line[line.index(after: colon)...].trimmingCharacters(in: .whitespaces)
            }
        }
        return nil
    }
}

private enum LoopbackError: Error {
    case socket(String, Int32)
    case timedOut(String)
    case closed
}

/// A one-connection HTTP/1.1 server on 127.0.0.1 with an ephemeral port. The
/// test writes the response itself, so it controls exactly when each byte of
/// the chunked body leaves the server. Blocking socket calls run on a
/// dispatch queue, never on the Swift concurrency pool.
private final class LoopbackHTTPServer: @unchecked Sendable {
    let port: UInt16
    private let lock = NSLock()
    private var listener: Int32
    private var connection: Int32 = -1
    private let queue = DispatchQueue(label: "LoopbackHTTPServer")

    init() throws {
        #if canImport(Darwin)
        let descriptor = socket(AF_INET, SOCK_STREAM, 0)
        #else
        let descriptor = socket(AF_INET, Int32(SOCK_STREAM.rawValue), 0)
        #endif
        guard descriptor >= 0 else { throw LoopbackError.socket("socket", errno) }
        var address = sockaddr_in()
        #if canImport(Darwin)
        address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
        #endif
        address.sin_family = sa_family_t(AF_INET)
        address.sin_port = 0
        address.sin_addr.s_addr = inet_addr("127.0.0.1")
        var length = socklen_t(MemoryLayout<sockaddr_in>.size)
        let bound = withUnsafeMutablePointer(to: &address) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { socketAddress in
                bind(descriptor, socketAddress, length) == 0
                    && listen(descriptor, 1) == 0
                    && getsockname(descriptor, socketAddress, &length) == 0
            }
        }
        guard bound else {
            let code = errno
            _ = systemClose(descriptor)
            throw LoopbackError.socket("bind", code)
        }
        listener = descriptor
        port = UInt16(bigEndian: address.sin_port)
    }

    /// Accepts the client's connection and reads one request: its head and a
    /// `Content-Length` body.
    func acceptRequest(timeout: TimeInterval) async throws -> LoopbackRequest {
        try await offPool { [self] in
            let listener = lock.withLock { self.listener }
            guard Self.waitReadable(listener, timeout: timeout) else { throw LoopbackError.timedOut("accept") }
            let accepted = accept(listener, nil, nil)
            guard accepted >= 0 else { throw LoopbackError.socket("accept", errno) }
            #if canImport(Darwin)
            var noSignal: Int32 = 1
            setsockopt(accepted, SOL_SOCKET, SO_NOSIGPIPE, &noSignal, socklen_t(MemoryLayout<Int32>.size))
            #endif
            lock.withLock { connection = accepted }

            var received = Data()
            let separator = Data("\r\n\r\n".utf8)
            var headEnd = received.range(of: separator)
            while headEnd == nil {
                received.append(try Self.read(accepted, timeout: timeout))
                headEnd = received.range(of: separator)
            }
            guard let headEnd,
                  let head = String(bytes: received[..<headEnd.lowerBound], encoding: .utf8) else {
                throw LoopbackError.socket("request head", 0)
            }
            var body = Data(received[headEnd.upperBound...])
            let partial = LoopbackRequest(head: head, body: Data())
            let length = partial.header("content-length").flatMap(Int.init) ?? 0
            while body.count < length {
                body.append(try Self.read(accepted, timeout: timeout))
            }
            return LoopbackRequest(head: head, body: body)
        }
    }

    func send(_ data: Data) throws {
        let descriptor = lock.withLock { connection }
        guard descriptor >= 0 else { throw LoopbackError.closed }
        try data.withUnsafeBytes { buffer in
            var offset = 0
            while offset < buffer.count {
                #if canImport(Darwin)
                let flags: Int32 = 0
                #else
                let flags = Int32(MSG_NOSIGNAL)
                #endif
                let written = systemSend(descriptor, buffer.baseAddress! + offset, buffer.count - offset, flags)
                guard written > 0 else { throw LoopbackError.socket("send", errno) }
                offset += written
            }
        }
    }

    /// Writes one HTTP/1.1 chunk holding `text`.
    func sendChunk(_ text: String) throws {
        let bytes = Data(text.utf8)
        var chunk = Data((String(bytes.count, radix: 16) + "\r\n").utf8)
        chunk.append(bytes)
        chunk.append(Data("\r\n".utf8))
        try send(chunk)
    }

    /// Waits until the client closes or resets the connection.
    func waitForPeerClose(timeout: TimeInterval) async -> Bool {
        (try? await offPool { [self] in
            let descriptor = lock.withLock { connection }
            guard descriptor >= 0 else { return false }
            let deadline = Date().addingTimeInterval(timeout)
            while Date() < deadline {
                guard Self.waitReadable(descriptor, timeout: deadline.timeIntervalSinceNow) else { return false }
                do {
                    _ = try Self.read(descriptor, timeout: 0)
                } catch LoopbackError.closed {
                    return true
                } catch LoopbackError.socket {
                    return true
                } catch LoopbackError.timedOut {
                    // A spurious wakeup; keep waiting until the deadline.
                    continue
                }
            }
            return false
        }) ?? false
    }

    func close() {
        let (listener, connection) = lock.withLock {
            let descriptors = (self.listener, self.connection)
            self.listener = -1
            self.connection = -1
            return descriptors
        }
        if connection >= 0 { _ = systemClose(connection) }
        if listener >= 0 { _ = systemClose(listener) }
    }

    private func offPool<T: Sendable>(_ body: @escaping @Sendable () throws -> T) async throws -> T {
        try await withCheckedThrowingContinuation { continuation in
            queue.async { continuation.resume(with: Result { try body() }) }
        }
    }

    private static func waitReadable(_ descriptor: Int32, timeout: TimeInterval) -> Bool {
        var request = pollfd(fd: descriptor, events: Int16(POLLIN), revents: 0)
        let milliseconds = Int32(max(0, min(timeout, 60)) * 1000)
        return poll(&request, 1, milliseconds) > 0
    }

    private static func read(_ descriptor: Int32, timeout: TimeInterval) throws -> Data {
        guard waitReadable(descriptor, timeout: timeout) else { throw LoopbackError.timedOut("read") }
        var buffer = [UInt8](repeating: 0, count: 4096)
        let count = recv(descriptor, &buffer, buffer.count, 0)
        if count == 0 { throw LoopbackError.closed }
        guard count > 0 else { throw LoopbackError.socket("recv", errno) }
        return Data(buffer[..<count])
    }
}

/// The C `close` and `send`, which the server's own methods of those names shadow.
private func systemClose(_ descriptor: Int32) -> Int32 {
    #if canImport(Darwin)
    Darwin.close(descriptor)
    #else
    Glibc.close(descriptor)
    #endif
}

/// The C `send`; see `systemClose`.
private func systemSend(_ descriptor: Int32, _ bytes: UnsafeRawPointer, _ count: Int, _ flags: Int32) -> Int {
    #if canImport(Darwin)
    Darwin.send(descriptor, bytes, count, flags)
    #else
    Glibc.send(descriptor, bytes, count, flags)
    #endif
}
