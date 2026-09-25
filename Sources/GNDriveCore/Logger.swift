// Structured logging — port of internal/logging/logger.go.
// Foreground mode writes text lines to stderr; service mode writes JSON.
import Foundation

public enum LogMode: String, Sendable {
    case foreground
    case service
}

public enum LogLevel: Int, Sendable, Comparable {
    case debug = 0, info, warn, error
    public static func < (a: LogLevel, b: LogLevel) -> Bool { a.rawValue < b.rawValue }
    var label: String {
        switch self {
        case .debug: return "DEBUG"
        case .info: return "INFO"
        case .warn: return "WARN"
        case .error: return "ERROR"
        }
    }
}

public final class Logger: @unchecked Sendable {
    public let mode: LogMode
    private let lock = NSLock()
    private var contextFields: [(String, String)] = []

    public init(mode: LogMode = .foreground) {
        self.mode = mode
    }

    public func with(_ key: String, _ value: String) -> Logger {
        let child = Logger(mode: mode)
        child.contextFields = contextFields + [(key, value)]
        return child
    }

    private func log(_ level: LogLevel, _ msg: String, _ fields: [(String, String)]) {
        let all = contextFields + fields
        lock.lock()
        defer { lock.unlock() }
        let ts = ISO8601DateFormatter().string(from: Date())
        if mode == .service {
            var dict: [String: Any] = ["time": ts, "level": level.label, "msg": msg]
            for (k, v) in all { dict[k] = v }
            if let data = try? JSONSerialization.data(withJSONObject: dict),
               let line = String(data: data, encoding: .utf8) {
                FileHandle.standardError.write(Data(line.utf8))
                FileHandle.standardError.write(Data("\n".utf8))
            }
        } else {
            var line = "\(ts) \(level.label) \(msg)"
            for (k, v) in all { line += " \(k)=\(v)" }
            FileHandle.standardError.write(Data((line + "\n").utf8))
        }
    }

    public func debug(_ msg: String, _ fields: (String, String)...) { log(.debug, msg, fields) }
    public func info(_ msg: String, _ fields: (String, String)...) { log(.info, msg, fields) }
    public func warn(_ msg: String, _ fields: (String, String)...) { log(.warn, msg, fields) }
    public func error(_ msg: String, _ fields: (String, String)...) { log(.error, msg, fields) }
    public func error(_ msg: String, err: Error, _ fields: (String, String)...) {
        log(.error, msg, fields + [("err", String(describing: err))])
    }
    public func warn(_ msg: String, err: Error, _ fields: (String, String)...) {
        log(.warn, msg, fields + [("err", String(describing: err))])
    }
}
