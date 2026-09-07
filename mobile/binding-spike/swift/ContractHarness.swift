import Dispatch
import Foundation
import RundaleBindingSpike

private struct CallbackRecord: @unchecked Sendable {
    let kind: rd_event_kind_t
    let request: rd_request_id_t
    let payload: String
    let callbackOnMainThread: Bool
}

@MainActor
private final class MainActorSink {
    private(set) var records: [CallbackRecord] = []
    private(set) var allDeliveriesOnMainThread = true

    func receive(_ record: CallbackRecord) {
        MainActor.preconditionIsolated()
        records.append(record)
        allDeliveriesOnMainThread = allDeliveriesOnMainThread && Thread.isMainThread
    }
}

private final class CallbackState: @unchecked Sendable {
    let sink: MainActorSink
    private let lock = NSLock()
    private var records: [CallbackRecord] = []
    private var terminal = false

    init(sink: MainActorSink) {
        self.sink = sink
    }

    func record(_ record: CallbackRecord) {
        lock.lock()
        records.append(record)
        if record.kind == RD_EVENT_COMPLETED || record.kind == RD_EVENT_LATE_IGNORED {
            terminal = true
        }
        lock.unlock()

        Task { @MainActor [sink] in
            sink.receive(record)
        }
    }

    func hasTerminal() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return terminal
    }

    func snapshot() -> [CallbackRecord] {
        lock.lock()
        defer { lock.unlock() }
        return records
    }
}

private func callbackEntry(
    _ session: rd_session_handle_t,
    _ request: rd_request_id_t,
    _ kind: rd_event_kind_t,
    _ payload: rd_bytes_t,
    _ context: UnsafeMutableRawPointer?
) {
    guard let context else { return }
    let state = Unmanaged<CallbackState>.fromOpaque(context).takeUnretainedValue()
    let text: String
    if let pointer = payload.ptr {
        text = String(decoding: UnsafeBufferPointer(start: pointer, count: payload.len), as: UTF8.self)
    } else {
        text = ""
    }
    state.record(
        CallbackRecord(
            kind: kind,
            request: request,
            payload: text,
            callbackOnMainThread: Thread.isMainThread
        )
    )
}

private enum HarnessError: Error, CustomStringConvertible {
    case failed(String)
    case timeout(String)

    var description: String {
        switch self {
        case let .failed(message): return message
        case let .timeout(message): return "timeout: \(message)"
        }
    }
}

private final class Harness: @unchecked Sendable {
    private(set) var assertions = 0
    private(set) var groups = 0

    func assert(_ condition: @autoclosure () -> Bool, _ message: String) throws {
        assertions += 1
        guard condition() else {
            throw HarnessError.failed(message)
        }
    }

    func status(
        _ actual: rd_status_t,
        equals expected: rd_status_t,
        _ message: String
    ) throws {
        try assert(actual == expected, "\(message): got \(actual), expected \(expected)")
    }

    func group(_ name: String, _ body: () throws -> Void) throws {
        groups += 1
        try body()
        print("PASS \(name)")
    }

    func groupAsync(_ name: String, _ body: @Sendable () async throws -> Void) async throws {
        groups += 1
        try await body()
        print("PASS \(name)")
    }

    func createSession() throws -> rd_session_handle_t {
        var session: rd_session_handle_t = 0
        try status(
            rd_session_create(&session),
            equals: RD_STATUS_OK,
            "create session"
        )
        try assert(session != 0, "create returned a zero handle")
        return session
    }

    func dispose(_ session: rd_session_handle_t) throws {
        try status(rd_session_dispose(session), equals: RD_STATUS_OK, "dispose session")
    }

    func withBytes<T>(_ text: String, _ body: (rd_bytes_t) throws -> T) rethrows -> T {
        let data = Data(text.utf8)
        return try data.withUnsafeBytes { rawBuffer in
            let pointer = rawBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self)
            return try body(rd_bytes_t(ptr: pointer, len: data.count))
        }
    }

    func takeBatch(
        _ session: rd_session_handle_t,
        maxBytes: Int,
        expected: rd_status_t = RD_STATUS_OK
    ) throws -> Data {
        var owned = rd_owned_bytes_t(ptr: nil, len: 0)
        let status = rd_session_take_json_batch(session, maxBytes, &owned)
        try self.status(status, equals: expected, "take JSON batch")
        guard expected == RD_STATUS_OK else { return Data() }
        defer {
            // The status is checked in the harness as well; a failed free is
            // a contract failure rather than a reason to leak test buffers.
            _ = rd_owned_bytes_free(owned)
        }
        guard let pointer = owned.ptr else { return Data() }
        return Data(bytes: pointer, count: owned.len)
    }

    func parseBatch(_ data: Data) throws -> [[String: Any]] {
        let object = try JSONSerialization.jsonObject(with: data)
        guard let batch = object as? [[String: Any]] else {
            throw HarnessError.failed("batch was not a JSON array of objects")
        }
        return batch
    }

    func waitUntil(
        _ description: String,
        timeoutNanoseconds: UInt64 = 2_000_000_000,
        _ predicate: @escaping @Sendable () async -> Bool
    ) async throws {
        let started = DispatchTime.now().uptimeNanoseconds
        while !(await predicate()) {
            if DispatchTime.now().uptimeNanoseconds - started > timeoutNanoseconds {
                throw HarnessError.timeout(description)
            }
            try await Task.sleep(nanoseconds: 5_000_000)
        }
    }

    func testOwnedUnicodeAndErrors() throws {
        let session = try createSession()
        let input = "céad 🌧️ — café \"quoted\""
        var request: rd_request_id_t = 0
        try withBytes(input) { bytes in
            try status(
                rd_session_submit_utf8(session, bytes, &request),
                equals: RD_STATUS_OK,
                "submit Unicode input"
            )
        }
        try assert(request == 1, "first request did not get stable ID 1")
        let batch = try takeBatch(session, maxBytes: 2048)
        let objects = try parseBatch(batch)
        try assert(objects.count == 1, "accepted batch had unexpected count")
        try assert(objects[0]["input"] as? String == input, "owned UTF-8 round trip changed Unicode")

        let invalid: [UInt8] = [0x66, 0x80]
        let invalidStatus = invalid.withUnsafeBufferPointer { buffer in
            rd_session_submit_utf8(
                session,
                rd_bytes_t(ptr: buffer.baseAddress, len: buffer.count),
                &request
            )
        }
        try status(invalidStatus, equals: RD_STATUS_INVALID_UTF8, "invalid UTF-8")
        try status(
            rd_session_submit_utf8(session, rd_bytes_t(ptr: nil, len: 1), &request),
            equals: RD_STATUS_INVALID_ARGUMENT,
            "null input pointer"
        )

        let oversized = String(repeating: "x", count: 5_000)
        try withBytes(oversized) { bytes in
            try status(
                rd_session_submit_utf8(session, bytes, &request),
                equals: RD_STATUS_TOO_LARGE,
                "input ceiling"
            )
        }
        try status(
            rd_session_complete(session, 999, rd_bytes_t(ptr: nil, len: 0)),
            equals: RD_STATUS_NOT_FOUND,
            "unknown request"
        )
        try status(
            rd_session_take_json_batch(0xfeed, 128, nil),
            equals: RD_STATUS_INVALID_ARGUMENT,
            "null output pointer is rejected before handle lookup"
        )
        try dispose(session)
        try withBytes("after dispose") { bytes in
            try status(
                rd_session_submit_utf8(session, bytes, &request),
                equals: RD_STATUS_INVALID_HANDLE,
                "disposed handle"
            )
        }
        var disposedBatch = rd_owned_bytes_t(ptr: nil, len: 0)
        try status(
            rd_session_take_json_batch(session, 128, &disposedBatch),
            equals: RD_STATUS_INVALID_HANDLE,
            "disposed handle batch"
        )
    }

    func testBoundedJSONBatch() throws {
        let session = try createSession()
        for index in 0..<40 {
            var request: rd_request_id_t = 0
            try withBytes("entry-\(index)-\(String(repeating: "x", count: 64))") { bytes in
                try status(
                    rd_session_submit_utf8(session, bytes, &request),
                    equals: RD_STATUS_OK,
                    "batch input \(index)"
                )
            }
        }
        let small = try takeBatch(session, maxBytes: 512)
        try assert(small.count <= 512, "bounded batch exceeded requested byte limit")
        let smallObjects = try parseBatch(small)
        try assert(!smallObjects.isEmpty, "bounded batch discarded all available events")
        let remainder = try takeBatch(session, maxBytes: 64 * 1024)
        let remainderObjects = try parseBatch(remainder)
        try assert(smallObjects.count + remainderObjects.count == 40, "batch paging lost events")
        try status(
            rd_session_take_json_batch(session, 1, nil),
            equals: RD_STATUS_INVALID_ARGUMENT,
            "batch minimum size"
        )
        try dispose(session)
    }

    func testRepeatedCreateDispose() throws {
        for _ in 0..<128 {
            let session = try createSession()
            try dispose(session)
            try status(
                rd_session_dispose(session),
                equals: RD_STATUS_INVALID_HANDLE,
                "repeated dispose remains invalid"
            )
            var batch = rd_owned_bytes_t(ptr: nil, len: 0)
            try status(
                rd_session_take_json_batch(session, 128, &batch),
                equals: RD_STATUS_INVALID_HANDLE,
                "disposed session remains invalid"
            )
        }
    }

    func submitAsync(
        _ session: rd_session_handle_t,
        input: String,
        state: CallbackState
    ) throws -> (rd_request_id_t, Unmanaged<CallbackState>) {
        let retained = Unmanaged.passRetained(state)
        var request: rd_request_id_t = 0
        do {
            try withBytes(input) { bytes in
                try status(
                    rd_session_submit_async(
                        session,
                        bytes,
                        callbackEntry,
                        retained.toOpaque(),
                        &request
                    ),
                    equals: RD_STATUS_OK,
                    "submit async"
                )
            }
        } catch {
            retained.release()
            throw error
        }
        return (request, retained)
    }

    func testCancellationAndLateResult() async throws {
        let session = try createSession()
        let sink = await MainActor.run { MainActorSink() }
        let state = CallbackState(sink: sink)
        let (request, retained) = try submitAsync(session, input: "stop me before commit", state: state)
        try await waitUntil("provisional callback") {
            state.snapshot().contains { $0.kind == RD_EVENT_PROVISIONAL }
        }
        try status(
            rd_session_cancel(session, request),
            equals: RD_STATUS_OK,
            "cancel active request"
        )
        try status(
            rd_session_cancel(session, request),
            equals: RD_STATUS_ALREADY_CANCELLED,
            "repeat cancel"
        )
        try await waitUntil("late callback after cancellation") { state.hasTerminal() }
        try await waitUntil("MainActor callback delivery") {
            await MainActor.run { sink.records.count == state.snapshot().count }
        }
        let callbacks = state.snapshot()
        try assert(callbacks.contains { $0.kind == RD_EVENT_PROVISIONAL }, "missing provisional callback")
        try assert(callbacks.contains { $0.kind == RD_EVENT_LATE_IGNORED }, "missing late callback")
        try assert(!callbacks.contains { $0.kind == RD_EVENT_COMPLETED }, "late result was reported as completed")
        try assert(callbacks.allSatisfy { !$0.callbackOnMainThread }, "Rust callback unexpectedly ran on main thread")
        let mainDelivery = await MainActor.run { (sink.records.count, sink.allDeliveriesOnMainThread) }
        try assert(mainDelivery.0 == callbacks.count, "callback delivery count changed during actor hop")
        try assert(mainDelivery.1, "callback hop did not execute on MainActor/main thread")
        try withBytes("late output must not commit") { bytes in
            try status(
                rd_session_complete(session, request, bytes),
                equals: RD_STATUS_ALREADY_CANCELLED,
                "late completion rejected after cancellation"
            )
        }
        let batch = try parseBatch(try takeBatch(session, maxBytes: 4096))
        try assert(batch.contains { $0["kind"] as? String == "cancelled" }, "cancel event missing")
        try assert(batch.contains { $0["kind"] as? String == "late_result_ignored" }, "late ignore event missing")
        try assert(!batch.contains { $0["kind"] as? String == "completed" }, "cancelled request committed a result")
        try dispose(session)
        retained.release()
    }

    func testBackgroundCompletionAndMainActorDelivery() async throws {
        let session = try createSession()
        let sink = await MainActor.run { MainActorSink() }
        let state = CallbackState(sink: sink)
        let (_, retained) = try submitAsync(session, input: "unicode completion: 你好", state: state)
        try await waitUntil("completed callback") { state.hasTerminal() }
        try await waitUntil("MainActor completion delivery") {
            await MainActor.run { sink.records.count == state.snapshot().count }
        }
        let callbacks = state.snapshot()
        try assert(callbacks.contains { $0.kind == RD_EVENT_PROVISIONAL }, "completion missing provisional callback")
        try assert(callbacks.contains { $0.kind == RD_EVENT_COMPLETED }, "completion missing terminal callback")
        try assert(callbacks.allSatisfy { !$0.callbackOnMainThread }, "background callback ran on main thread")
        let mainDelivery = await MainActor.run { (sink.records.count, sink.allDeliveriesOnMainThread) }
        try assert(mainDelivery.0 == callbacks.count, "completion callback delivery count mismatch")
        try assert(mainDelivery.1, "completion callback was not delivered on MainActor")
        try dispose(session)
        retained.release()
    }
}

@main
private struct BindingSpikeHarness {
    static func main() async {
        let harness = Harness()
        do {
            try harness.group("owned UTF-8, errors, and disposed handles") {
                try harness.testOwnedUnicodeAndErrors()
            }
            try harness.group("bounded JSON event batches") {
                try harness.testBoundedJSONBatch()
            }
            try harness.group("repeated create/dispose") {
                try harness.testRepeatedCreateDispose()
            }
            try await harness.groupAsync("cancellation and late result rejection") {
                try await harness.testCancellationAndLateResult()
            }
            try await harness.groupAsync("background callback and MainActor delivery") {
                try await harness.testBackgroundCompletionAndMainActorDelivery()
            }
            print("RESULT status=pass groups=\(harness.groups) assertions=\(harness.assertions)")
        } catch {
            print("RESULT status=fail groups=\(harness.groups) assertions=\(harness.assertions) error=\(error)")
            Foundation.exit(1)
        }
    }
}
