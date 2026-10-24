// In-memory ring buffer fed by Logger.sink for logs.read / log:line.
import Foundation

public final class RPCLogBuffer: @unchecked Sendable {
    public struct Line: Sendable {
        public var time: Date
        public var level: LogLevel
        public var msg: String
        public var fields: [String: String]
    }

    private let lock = NSLock()
    private let capacity: Int
    private var lines: [Line] = []
    private var dropped = 0

    /// Called for every accepted line; the serve bridge forwards it as
    /// `log:line`. Kept out of the buffer lock so a slow writer cannot stall
    /// logging.
    public var onLine: ((LogRecord) -> Void)?

    public init(capacity: Int = 2000) {
        self.capacity = max(1, capacity)
        lines.reserveCapacity(min(capacity, 4096))
    }

    public func append(_ record: LogRecord) {
        lock.lock()
        lines.append(Line(time: record.time, level: record.level,
                          msg: record.msg, fields: record.fields))
        if lines.count > capacity {
            let excess = lines.count - capacity
            lines.removeFirst(excess)
            dropped += excess
        }
        lock.unlock()
        onLine?(record)
    }

    public func read(limit: Int, minLevel: LogLevel?, since: Int64?) -> (lines: [Line], dropped: Int) {
        lock.lock()
        defer { lock.unlock() }
        var selected = lines
        if let minLevel {
            selected = selected.filter { $0.level >= minLevel }
        }
        if let since {
            selected = selected.filter { RPCJSON.millis($0.time) > since }
        }
        if selected.count > limit {
            selected = Array(selected.suffix(limit))
        }
        return (selected, dropped)
    }

    public func encode(_ line: Line) -> [String: Any] {
        [
            "time": RPCJSON.rfc3339(line.time),
            "level": line.level.label,
            "msg": line.msg,
            "fields": line.fields,
        ]
    }
}
