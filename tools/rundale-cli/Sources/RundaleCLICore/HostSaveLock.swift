import Foundation
#if os(Linux)
import Glibc
#else
import Darwin
#endif

/// A kernel-owned lock survives stale lock files and releases on process death.
final class HostSaveLock {
    private var descriptor: Int32 = -1

    init(directory: URL) throws {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        descriptor = open(directory.appendingPathComponent(".session-host.lock").path, O_CREAT | O_RDWR, 0o600)
        guard descriptor >= 0 else { throw HostCommandError.saveLocked }
        guard flock(descriptor, LOCK_EX | LOCK_NB) == 0 else {
            close(descriptor)
            descriptor = -1
            throw HostCommandError.saveLocked
        }
    }

    deinit { if descriptor >= 0 { close(descriptor) } }
}
