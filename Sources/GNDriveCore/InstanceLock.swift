// Single-instance advisory locking — port of internal/instance/lock.go.
// Lock file: <configDir>/gn-drive.lock (flock); PID file: gn-drive.pid.
import Foundation
import Darwin

public enum InstanceError: Error, LocalizedError {
    case anotherInstance(pid: Int32?)

    public var errorDescription: String? {
        switch self {
        case .anotherInstance(let pid):
            if let pid {
                return "another gn-drive instance is running (pid=\(pid)). Run 'gn-drive service stop' to stop it, or kill the process manually"
            }
            return "another gn-drive instance is running. Run 'gn-drive service stop' to stop it, or kill the process manually"
        }
    }
}

public final class InstanceLocker {
    private let fd: Int32
    private let pidFile: String
    private let pid: Int32
    private var released = false
    private let lock = NSLock()

    /// Take an exclusive advisory lock on the config dir.
    public static func acquire(configDir: String) throws -> InstanceLocker {
        try FileManager.default.createDirectory(atPath: configDir, withIntermediateDirectories: true)
        let lockPath = (configDir as NSString).appendingPathComponent("gn-drive.lock")
        let pidPath = (configDir as NSString).appendingPathComponent("gn-drive.pid")

        let fd = open(lockPath, O_CREAT | O_RDWR, 0o600)
        guard fd >= 0 else { throw CocoaError(.fileWriteUnknown) }
        if flock(fd, LOCK_EX | LOCK_NB) != 0 {
            close(fd)
            let existing = readPID(pidPath)
            throw InstanceError.anotherInstance(pid: existing > 0 ? existing : nil)
        }
        let pid = getpid()
        try "\(pid)".write(toFile: pidPath, atomically: true, encoding: .utf8)
        try? FileManager.default.setAttributes([.posixPermissions: 0o644], ofItemAtPath: pidPath)
        return InstanceLocker(fd: fd, pidFile: pidPath, pid: pid)
    }

    private init(fd: Int32, pidFile: String, pid: Int32) {
        self.fd = fd; self.pidFile = pidFile; self.pid = pid
    }

    public var pidValue: Int32 { pid }

    public func release() {
        lock.lock(); defer { lock.unlock() }
        if released { return }
        released = true
        if let data = try? String(contentsOfFile: pidFile, encoding: .utf8),
           Int32(data.trimmingCharacters(in: .whitespacesAndNewlines)) == pid {
            try? FileManager.default.removeItem(atPath: pidFile)
        }
        flock(fd, LOCK_UN)
        close(fd)
    }

    deinit { release() }

    private static func readPID(_ path: String) -> Int32 {
        guard let s = try? String(contentsOfFile: path, encoding: .utf8) else { return 0 }
        return Int32(s.trimmingCharacters(in: .whitespacesAndNewlines)) ?? 0
    }
}
