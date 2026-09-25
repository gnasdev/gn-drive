// Thin synchronous SQLite3 wrapper — replaces modernc.org/sqlite + database/sql.
// All access is serialized on a private queue (SQLite single-writer).
import Foundation
import SQLite3

public enum DatabaseError: Error {
    case open(String)
    case prepare(String)
    case step(String)
    case bind(String)
    case notFound
}

public enum SQLValue: Sendable {
    case null
    case int(Int64)
    case double(Double)
    case text(String)
    case blob(Data)

    static func from(_ v: Any?) -> SQLValue {
        switch v {
        case nil, is NSNull: return .null
        case let i as Int: return .int(Int64(i))
        case let i as Int64: return .int(i)
        case let d as Double: return .double(d)
        case let s as String: return .text(s)
        case let b as Bool: return .int(b ? 1 : 0)
        case let d as Data: return .blob(d)
        default: return .text(String(describing: v!))
        }
    }
}

public final class Database: @unchecked Sendable {
    private var db: OpaquePointer?
    private let queue = DispatchQueue(label: "gn-drive.sqlite")
    public let path: String

    public init(path: String) throws {
        self.path = path
        try FileManager.default.createDirectory(
            atPath: (path as NSString).deletingLastPathComponent,
            withIntermediateDirectories: true)
        var handle: OpaquePointer?
        if sqlite3_open(path, &handle) != SQLITE_OK {
            let msg = handle.flatMap { String(cString: sqlite3_errmsg($0)) } ?? "unknown"
            sqlite3_close(handle)
            throw DatabaseError.open(msg)
        }
        self.db = handle
        try exec("PRAGMA journal_mode=WAL")
        try exec("PRAGMA foreign_keys=ON")
        try exec("PRAGMA busy_timeout=5000")
    }

    deinit { sqlite3_close(db) }

    public func close() {
        queue.sync { if db != nil { sqlite3_close(db); db = nil } }
    }

    /// Execute a statement without results.
    @discardableResult
    public func exec(_ sql: String, _ args: SQLValue...) throws -> Int64 {
        try queue.sync { try execLocked(sql, args) }
    }

    public func exec(_ sql: String, args: [SQLValue]) throws -> Int64 {
        try queue.sync { try execLocked(sql, args) }
    }

    /// Query rows. Each row is an array of SQLValue.
    public func query(_ sql: String, _ args: SQLValue...) throws -> [[SQLValue]] {
        try queue.sync { try queryLocked(sql, args) }
    }

    public func query(_ sql: String, args: [SQLValue]) throws -> [[SQLValue]] {
        try queue.sync { try queryLocked(sql, args) }
    }

    /// Query a single row or nil.
    public func queryRow(_ sql: String, _ args: SQLValue...) throws -> [SQLValue]? {
        try queue.sync { try queryLocked(sql, args).first }
    }

    /// Run a block inside a transaction.
    public func transaction<T>(_ body: () throws -> T) throws -> T {
        try queue.sync {
            try execLocked("BEGIN IMMEDIATE")
            do {
                let result = try body()
                try execLocked("COMMIT")
                return result
            } catch {
                try? execLocked("ROLLBACK")
                throw error
            }
        }
    }

    /// Unsafe: runs body while holding the DB queue — only for transaction bodies.
    public func execInTransaction(_ sql: String, _ args: SQLValue...) throws -> Int64 {
        try execLocked(sql, args)
    }

    // --- private (must be called on queue) -----------------------------------

    private func execLocked(_ sql: String, _ args: [SQLValue] = []) throws -> Int64 {
        let stmt = try prepareLocked(sql)
        defer { sqlite3_finalize(stmt) }
        try bindLocked(stmt, args)
        let rc = sqlite3_step(stmt)
        guard rc == SQLITE_DONE || rc == SQLITE_ROW else {
            throw DatabaseError.step(errmsg() + " (sql: \(sql.prefix(80)))")
        }
        return sqlite3_changes64(db)
    }

    private func queryLocked(_ sql: String, _ args: [SQLValue] = []) throws -> [[SQLValue]] {
        let stmt = try prepareLocked(sql)
        defer { sqlite3_finalize(stmt) }
        try bindLocked(stmt, args)
        var rows: [[SQLValue]] = []
        while true {
            let rc = sqlite3_step(stmt)
            if rc == SQLITE_DONE { break }
            guard rc == SQLITE_ROW else {
                throw DatabaseError.step(errmsg() + " (sql: \(sql.prefix(80)))")
            }
            let n = Int(sqlite3_column_count(stmt))
            var row = [SQLValue](repeating: .null, count: Int(n))
            for i in 0..<n {
                let idx = Int32(i)
                switch sqlite3_column_type(stmt, idx) {
                case SQLITE_INTEGER:
                    row[i] = .int(sqlite3_column_int64(stmt, idx))
                case SQLITE_FLOAT:
                    row[i] = .double(sqlite3_column_double(stmt, idx))
                case SQLITE_TEXT:
                    row[i] = .text(String(cString: sqlite3_column_text(stmt, idx)))
                case SQLITE_BLOB:
                    let len = sqlite3_column_bytes(stmt, idx)
                    row[i] = .blob(Data(bytes: sqlite3_column_blob(stmt, idx), count: Int(len)))
                default:
                    row[i] = .null
                }
            }
            rows.append(row)
        }
        return rows
    }

    private func prepareLocked(_ sql: String) throws -> OpaquePointer {
        var stmt: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK else {
            throw DatabaseError.prepare(errmsg() + " (sql: \(sql.prefix(80)))")
        }
        return stmt!
    }

    private func bindLocked(_ stmt: OpaquePointer, _ args: [SQLValue]) throws {
        for (i, v) in args.enumerated() {
            let idx = Int32(i + 1)
            let rc: Int32
            switch v {
            case .null: rc = sqlite3_bind_null(stmt, idx)
            case .int(let x): rc = sqlite3_bind_int64(stmt, idx, x)
            case .double(let x): rc = sqlite3_bind_double(stmt, idx, x)
            case .text(let s):
                rc = sqlite3_bind_text(stmt, idx, s, -1, unsafeBitCast(-1, to: sqlite3_destructor_type.self))
            case .blob(let d):
                rc = d.withUnsafeBytes { ptr in
                    sqlite3_bind_blob(stmt, idx, ptr.baseAddress, Int32(d.count), unsafeBitCast(-1, to: sqlite3_destructor_type.self))
                }
            }
            guard rc == SQLITE_OK else { throw DatabaseError.bind(errmsg()) }
        }
    }

    private func errmsg() -> String {
        String(cString: sqlite3_errmsg(db))
    }
}

extension SQLValue {
    public var text: String {
        switch self {
        case .text(let s): return s
        case .int(let i): return String(i)
        case .double(let d): return String(d)
        default: return ""
        }
    }
    public var int: Int64 {
        switch self {
        case .int(let i): return i
        case .double(let d): return Int64(d)
        case .text(let s): return Int64(s) ?? 0
        default: return 0
        }
    }
    public var double: Double {
        switch self {
        case .double(let d): return d
        case .int(let i): return Double(i)
        default: return 0
        }
    }
    public var isNull: Bool { if case .null = self { return true }; return false }
}
