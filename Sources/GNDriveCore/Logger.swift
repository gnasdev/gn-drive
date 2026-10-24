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
    public var label: String {
        switch self {
        case .debug: return "DEBUG"
        case .info: return "INFO"
        case .warn: return "WARN"
        case .error: return "ERROR"
        }
    }
    public init?(label: String) {
        switch label.lowercased() {
        case "debug": self = .debug
        case "info": self = .info
        case "warn", "warning": self = .warn
        case "error": self = .error
        default: return nil
        }
    }
}

/// One emitted log line, fed to an optional sink (serve's RPC ring buffer).
public struct LogRecord: Sendable {
    public var time: Date
    public var level: LogLevel
    public var msg: String
    public var fields: [String: String]
    public init(time: Date, level: LogLevel, msg: String, fields: [String: String]) {
        self.time = time; self.level = level; self.msg = msg; self.fields = fields
    }
}

public final class Logger: @unchecked Sendable {
    public let mode: LogMode
    private let lock = NSLock()
    private var contextFields: [(String, String)] = []
    private let shared: Shared

    /// State shared between a root logger and its `with(...)` children.
    final class Shared: @unchecked Sendable {
        let lock = NSLock()
        var minLevel: LogLevel = .debug
        var sink: ((LogRecord) -> Void)?
    }

    public init(mode: LogMode = .foreground) {
        self.mode = mode
        self.shared = Shared()
    }

    private init(mode: LogMode, fields: [(String, String)], shared: Shared) {
        self.mode = mode
        self.contextFields = fields
        self.shared = shared
    }

    public var minLevel: LogLevel {
        get { shared.lock.lock(); defer { shared.lock.unlock() }; return shared.minLevel }
        set { shared.lock.lock(); shared.minLevel = newValue; shared.lock.unlock() }
    }

    /// Optional line sink for the stdio RPC bridge. Runs on the logging thread
    /// after the stderr write; must be fast and non-blocking.
    public var sink: ((LogRecord) -> Void)? {
        get { shared.lock.lock(); defer { shared.lock.unlock() }; return shared.sink }
        set { shared.lock.lock(); shared.sink = newValue; shared.lock.unlock() }
    }

    public func with(_ key: String, _ value: String) -> Logger {
        Logger(mode: mode, fields: contextFields + [(key, value)], shared: shared)
    }

    private func log(_ level: LogLevel, _ msg: String, _ fields: [(String, String)]) {
        if level < minLevel { return }
        let all = contextFields + fields
        let now = Date()
        lock.lock()
        let ts = ISO8601DateFormatter().string(from: now)
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
        lock.unlock()

        let sink = shared.sink
        if let sink {
            var map: [String: String] = [:]
            for (k, v) in all { map[k] = v }
            sink(LogRecord(time: now, level: level, msg: msg, fields: map))
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
