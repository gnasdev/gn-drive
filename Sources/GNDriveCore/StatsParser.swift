// rclone log-line stats parsing — port of internal/rclone/stats.go.
import Foundation

public let maxTrackedFiles = 150

// MARK: - JSON log schema ("--use-json-log")

struct JSONLogStats: Decodable {
    var bytes: Int64 = 0
    var totalBytes: Int64 = 0
    var transfers: Int64 = 0
    var totalTransfers: Int64 = 0
    var checks: Int64 = 0
    var totalChecks: Int64 = 0
    var deletes: Int64 = 0
    var renames: Int64 = 0
    var errors: Int64 = 0
    var speed: Double = 0
    var eta: Double? = nil
    var transferring: [JSONTransferring] = []
}

struct JSONTransferring: Decodable {
    var name: String = ""
    var size: Int64 = 0
    var bytes: Int64 = 0
    var percentage: Int = 0
    var speed: Double = 0
}

struct JSONLogLine: Decodable {
    var level: String = ""
    var stats: JSONLogStats? = nil
    var object: String = ""
    var msg: String = ""
    var size: Int64 = 0
}

// MARK: - File transfer tracker

/// Accumulates per-file status from CLI JSON logs.
final class FileTransferTracker {
    private var byName: [String: FileTransfer] = [:]
    private var order: [String] = []

    func upsert(_ ft: FileTransfer) {
        guard !ft.name.isEmpty else { return }
        if let prev = byName[ft.name] {
            // Don't demote completed/failed/checked back to transferring.
            if ["completed", "failed", "checked"].contains(prev.status), ft.status == "transferring" {
                return
            }
            byName[ft.name] = ft
            return
        }
        if order.count >= maxTrackedFiles, ft.status != "failed" {
            evictReplaceable()
            if order.count >= maxTrackedFiles { return }
        }
        byName[ft.name] = ft
        order.append(ft.name)
    }

    private func evictReplaceable() {
        for (i, name) in order.enumerated() where byName[name]?.status == "pending" {
            byName.removeValue(forKey: name)
            order.remove(at: i)
            return
        }
        for (i, name) in order.enumerated() {
            if let s = byName[name]?.status, s == "completed" || s == "checked" {
                byName.removeValue(forKey: name)
                order.remove(at: i)
                return
            }
        }
    }

    /// Insert listed files as pending without demoting known live rows.
    func seedPending(_ entries: [FileEntry]) {
        for e in entries where !e.isDir {
            let name = !e.path.trimmingCharacters(in: .whitespaces).isEmpty
                ? e.path.trimmingCharacters(in: .whitespaces)
                : e.name.trimmingCharacters(in: .whitespaces)
            guard !name.isEmpty else { continue }
            if var prev = byName[name] {
                if prev.size == 0, e.size > 0 {
                    prev.size = e.size
                    byName[name] = prev
                }
                continue
            }
            if order.count >= maxTrackedFiles { continue }
            byName[name] = FileTransfer(name: name, size: e.size, status: "pending")
            order.append(name)
        }
    }

    func snapshot(totalFiles: Int64) -> [FileTransfer] {
        var out: [FileTransfer] = []
        var active = 0, completed = 0, failed = 0, pendingNamed = 0
        for name in order {
            guard let ft = byName[name] else { continue }
            out.append(ft)
            switch ft.status {
            case "transferring", "checking": active += 1
            case "completed", "checked": completed += 1
            case "failed": failed += 1
            case "pending": pendingNamed += 1
            default: break
            }
        }
        if totalFiles > 0 {
            let known = Int64(completed + failed + active + pendingNamed)
            let pend = totalFiles - known
            if pend > 0 {
                out.append(FileTransfer(name: "(\(pend) pending)", status: "pending"))
            }
        }
        return out
    }
}

// MARK: - Line parsers

enum StatsParser {
    /// Parse a JSON log line's "stats" object into s. Returns false when the
    /// line is not JSON-with-stats (caller falls back to text parser).
    static func parseJSONStatsLine(_ line: String, into s: inout SyncStats) -> Bool {
        let t = line.trimmingCharacters(in: .whitespaces)
        guard t.first == "{" else { return false }
        guard let data = t.data(using: .utf8),
              let entry = try? JSONDecoder().decode(JSONLogLine.self, from: data),
              let st = entry.stats else { return false }
        s.bytes = st.bytes
        s.bytesTotal = st.totalBytes
        s.files = st.transfers
        s.filesTotal = st.totalTransfers
        s.transfers = st.transfers
        s.checks = st.checks
        s.checksTotal = st.totalChecks
        s.deletes = st.deletes
        s.renames = st.renames
        s.errors = st.errors
        s.speed = st.speed
        if let eta = st.eta { s.eta = Int64(eta) }
        if !entry.object.isEmpty { s.currentFile = entry.object }
        return true
    }

    /// Update aggregate stats + per-file tracker from one log line.
    static func ingestJSONLogLine(_ line: String, into s: inout SyncStats, track: FileTransferTracker) {
        let t = line.trimmingCharacters(in: .whitespaces)
        guard t.first == "{", let data = t.data(using: .utf8),
              let entry = try? JSONDecoder().decode(JSONLogLine.self, from: data) else { return }

        if let st = entry.stats, !st.transferring.isEmpty {
            for tr in st.transferring where !tr.name.isEmpty {
                s.currentFile = tr.name
                track.upsert(FileTransfer(
                    name: tr.name, size: tr.size, bytes: tr.bytes,
                    progress: Double(tr.percentage), status: "transferring", speed: tr.speed))
            }
        }

        guard !entry.object.isEmpty else { return }
        s.currentFile = entry.object
        let msg = entry.msg.lowercased()
        let level = entry.level.lowercased()

        var ft = FileTransfer(name: entry.object, size: entry.size, bytes: entry.size)
        if level == "error" || msg.contains("error") || msg.contains("failed") {
            ft.status = "failed"
            ft.error = entry.msg
            ft.progress = 0
        } else if msg.contains("check") {
            if msg.contains("ok") || msg.contains("identical") {
                ft.status = "checked"
                ft.progress = 100
            } else {
                ft.status = "checking"
            }
        } else if msg.contains("copied") || msg.contains("moved") || msg.contains("updated")
                    || msg.contains("multi-thread") {
            ft.status = "completed"
            ft.progress = 100
            if entry.size > 0 { ft.bytes = entry.size }
        } else {
            if level == "info" || level == "notice" {
                ft.status = "completed"
                ft.progress = 100
            } else {
                return
            }
        }
        track.upsert(ft)
    }

    /// Reduce rclone messages into stable UI lifecycle markers.
    static func updateStage(from line: String, into s: inout SyncStats) {
        let t = line.trimmingCharacters(in: .whitespaces)
        guard t.first == "{", let data = t.data(using: .utf8),
              let entry = try? JSONDecoder().decode(JSONLogLine.self, from: data) else { return }
        let msg = entry.msg.lowercased()
        let stage: String
        if (entry.stats?.transferring.isEmpty == false) || msg.contains("copied")
            || msg.contains("transferred") || msg.contains("moved") {
            stage = "transferring"
        } else if msg.contains("checking") || msg.contains("check") {
            stage = "checking"
        } else if msg.contains("listing") || msg.contains("list ") {
            stage = "listing"
        } else if msg.contains("retry") || msg.contains("rate limit") || msg.contains("throttle") {
            stage = "retrying"
        } else if msg.contains("starting") || msg.contains("config file") || msg.contains("using ") {
            stage = "connecting"
        } else {
            return
        }
        s.stage = stage
        s.stageDetail = entry.object
    }

    /// Legacy text --stats parser ("TRANSFER: x/y" etc.).
    static func parseStatsLine(_ line: String, into s: inout SyncStats) {
        guard line.contains("INFO") else { return }
        if let i = line.range(of: "TRANSFER: ") {
            if let (a, b) = parseFraction(String(line[i.upperBound...])) {
                s.bytes = a; s.bytesTotal = b
            }
        }
        if let i = line.range(of: "CHECK: ") {
            if let (a, b) = parseFraction(String(line[i.upperBound...])) {
                s.checks = a; s.checksTotal = b
            }
        }
        if let i = line.range(of: "ERRORS: ") {
            if let n = parseLeadingInt(String(line[i.upperBound...])) { s.errors = n }
        }
        if let i = line.range(of: "DELETED: ") {
            if let n = parseLeadingInt(String(line[i.upperBound...])) { s.deletes = n }
        }
    }

    static func parseFraction(_ s: String) -> (Int64, Int64)? {
        guard let space = s.firstIndex(of: " ") else { return nil }
        let rest = s[s.index(after: space)...]
        guard let slash = rest.firstIndex(of: "/") else { return nil }
        let left = String(rest[rest.startIndex..<slash])
        let rightAndMore = rest[rest.index(after: slash)...]
        let right = rightAndMore.split(separator: " ").first.map(String.init) ?? String(rightAndMore)
        return (parseSize(left), parseSize(right))
    }

    static func parseLeadingInt(_ s: String) -> Int64? {
        let digits = s.dropFirst(s.firstIndex(of: " ").map { s.distance(from: s.startIndex, to: $0) + 1 } ?? 0)
            .prefix { $0.isNumber }
        return Int64(digits)
    }

    /// Parse rclone size suffixes: "1.024k", "2M", "1G", "1024" → bytes.
    static func parseSize(_ s: String) -> Int64 {
        var i = s.startIndex
        while i < s.endIndex, s[i].isNumber || s[i] == "." {
            i = s.index(after: i)
        }
        let numStr = String(s[s.startIndex..<i])
        let suffix = String(s[i...]).lowercased()
        guard let n = Double(numStr), !numStr.isEmpty else { return 0 }
        let mult: Double
        switch suffix {
        case "k", "kb": mult = 1024
        case "m", "mb": mult = 1024 * 1024
        case "g", "gb": mult = 1024 * 1024 * 1024
        case "t", "tb": mult = 1024 * 1024 * 1024 * 1024
        default: mult = 1
        }
        return Int64(n * mult)
    }
}

func unixNow() -> Int64 { Int64(Date().timeIntervalSince1970) }

func truncateString(_ s: String, _ n: Int) -> String {
    s.count <= n ? s : String(s.prefix(n)) + "...(truncated)"
}
