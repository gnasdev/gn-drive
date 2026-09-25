// Cron scheduling — port of robfig/cron v3 semantics used by syncengine.
// Supports 5-field (min hour dom mon dow), 6-field (with seconds), and
// @descriptors (@hourly, @daily, @weekly, @monthly, @yearly, @every <dur>).
import Foundation

public enum CronError: Error, LocalizedError {
    case empty
    case badFieldCount(Int)
    case invalid(String)

    public var errorDescription: String? {
        switch self {
        case .empty: return "cron expression is empty"
        case .badFieldCount(let n): return "invalid cron: want 5 or 6 fields, got \(n)"
        case .invalid(let s): return "invalid cron expression: \(s)"
        }
    }
}

struct CronField {
    // allowed values within [min, max]; nil means "all"
    var values: Set<Int>?

    func contains(_ v: Int) -> Bool { values?.contains(v) ?? true }

    /// Next allowed value strictly greater than v, wrapping to minimum + carry.
    func next(after v: Int, min: Int, max: Int) -> (value: Int, carried: Bool) {
        guard let values else {
            return v < max ? (v + 1, false) : (min, true)
        }
        if let n = values.sorted().first(where: { $0 > v }) { return (n, false) }
        return (values.min() ?? min, true)
    }
    /// First allowed value >= v; nil if none in [v, max].
    func firstAtOrAfter(_ v: Int) -> Int? {
        guard let values else { return v }
        return values.sorted().first(where: { $0 >= v })
    }
    var minimum: Int { values?.min() ?? 0 }
}

public struct CronSchedule: Sendable {
    var seconds: CronField
    var minutes: CronField
    var hours: CronField
    var dom: CronField
    var months: CronField
    var dow: CronField
    var domRestricted = false
    var dowRestricted = false
    /// For @every: fixed interval in seconds.
    var everySeconds: Int? = nil

    private static let monthNames = ["jan", "feb", "mar", "apr", "may", "jun",
                                     "jul", "aug", "sep", "oct", "nov", "dec"]
    private static let dowNames = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"]

    /// Accepts 5-field UI expressions or 6-field second-aware ones.
    /// 5-field input gets "0 " prepended (matching Go's NormalizeCron).
    public init(_ expr: String) throws {
        var e = expr.trimmingCharacters(in: .whitespaces)
        guard !e.isEmpty else { throw CronError.empty }

        if e.hasPrefix("@") {
            switch e.lowercased() {
            case "@yearly", "@annually": e = "0 0 0 1 1 *"
            case "@monthly": e = "0 0 0 1 * *"
            case "@weekly": e = "0 0 0 * * 0"
            case "@daily", "@midnight": e = "0 0 0 * * *"
            case "@hourly": e = "0 0 * * * *"
            default:
                if e.lowercased().hasPrefix("@every "),
                   let d = Self.parseDuration(String(e.dropFirst(7))) {
                    everySeconds = Int(d)
                    seconds = CronField(); minutes = CronField(); hours = CronField()
                    dom = CronField(); months = CronField(); dow = CronField()
                    return
                }
                throw CronError.invalid(expr)
            }
        }

        var fields = e.split(separator: " ").map(String.init)
        switch fields.count {
        case 5: fields.insert("0", at: 0)
        case 6: break
        default: throw CronError.badFieldCount(fields.count)
        }

        seconds = try Self.parseField(fields[0], min: 0, max: 59)
        minutes = try Self.parseField(fields[1], min: 0, max: 59)
        hours = try Self.parseField(fields[2], min: 0, max: 23)
        dom = try Self.parseField(fields[3], min: 1, max: 31)
        months = try Self.parseField(fields[4], min: 1, max: 12, names: Self.monthNames)
        dow = try Self.parseField(fields[5], min: 0, max: 7, names: Self.dowNames)
        // Normalize dow 7 → 0 (both Sunday).
        if dow.values?.contains(7) == true {
            dow.values?.remove(7)
            dow.values?.insert(0)
        }
        domRestricted = !(fields[3] == "*" || fields[3] == "?")
        dowRestricted = !(fields[5] == "*" || fields[5] == "?")
    }

    static func parseDuration(_ s: String) -> TimeInterval? {
        // "1h30m", "15m", "30s" etc.
        var total: TimeInterval = 0
        var num = ""
        for c in s {
            if c.isNumber || c == "." {
                num.append(c)
            } else {
                guard let v = Double(num) else { return nil }
                num = ""
                switch c {
                case "h": total += v * 3600
                case "m": total += v * 60
                case "s": total += v
                default: return nil
                }
            }
        }
        if !num.isEmpty, let v = Double(num) { total += v }
        return total > 0 ? total : nil
    }

    static func parseField(_ field: String, min: Int, max: Int, names: [String] = []) throws -> CronField {
        var f = field.trimmingCharacters(in: .whitespaces).lowercased()
        if f == "*" || f == "?" { return CronField(values: nil) }
        // Replace names with numbers (1-based for months, 0-based for dow)
        for (i, n) in names.enumerated() {
            f = f.replacingOccurrences(of: n, with: String(names == monthNames ? i + 1 : i))
        }
        var values = Set<Int>()
        for part in f.split(separator: ",") {
            let p = String(part)
            let (rangePart, step) = p.contains("/")
                ? (String(p.split(separator: "/")[0]), Int(p.split(separator: "/")[1]) ?? 1)
                : (p, 1)
            guard step > 0 else { throw CronError.invalid(field) }
            let lo: Int, hi: Int
            if rangePart == "*" || rangePart == "?" {
                lo = min; hi = max
            } else if rangePart.contains("-") {
                let bounds = rangePart.split(separator: "-").map(String.init)
                guard bounds.count == 2, let a = Int(bounds[0]), let b = Int(bounds[1]) else {
                    throw CronError.invalid(field)
                }
                lo = a; hi = b
            } else {
                guard let a = Int(rangePart) else { throw CronError.invalid(field) }
                // "N" alone means just N; "N/step" means N-max.
                lo = a
                hi = p.contains("/") ? max : a
            }
            guard lo >= min, hi <= max, lo <= hi else { throw CronError.invalid(field) }
            var v = lo
            while v <= hi { values.insert(v); v += step }
        }
        return CronField(values: values)
    }

    /// Next fire time strictly after `date`.
    public func next(after date: Date) -> Date? {
        if let every = everySeconds {
            return date.addingTimeInterval(TimeInterval(every))
        }
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = .current
        // Start at the next whole second.
        var t = Date(timeIntervalSince1970: floor(date.timeIntervalSince1970) + 1)
        let limit = cal.date(byAdding: .year, value: 4, to: date)!

        while t < limit {
            var comps = cal.dateComponents([.year, .month, .day, .hour, .minute, .second, .weekday], from: t)
            let month = comps.month ?? 1
            guard let m = months.firstAtOrAfter(month) else { return nil }
            if m != month {
                // Advance to first day of next allowed month.
                var c = DateComponents(); c.year = comps.year; c.month = m; c.day = 1
                t = cal.date(from: c)!
                continue
            }
            // Day match: dom OR dow when both restricted (standard cron).
            let dayOK: Bool
            if domRestricted && dowRestricted {
                dayOK = dom.contains(comps.day ?? 1) || dow.contains((comps.weekday ?? 1) - 1)
            } else if domRestricted {
                dayOK = dom.contains(comps.day ?? 1)
            } else if dowRestricted {
                dayOK = dow.contains((comps.weekday ?? 1) - 1)
            } else {
                dayOK = true
            }
            if !dayOK {
                t = cal.date(byAdding: .day, value: 1, to: cal.startOfDay(for: t))!
                continue
            }
            let hour = comps.hour ?? 0
            if !hours.contains(hour) {
                guard let h = hours.firstAtOrAfter(hour + 1) else {
                    t = cal.date(byAdding: .day, value: 1, to: cal.startOfDay(for: t))!
                    continue
                }
                var c = cal.dateComponents([.year, .month, .day], from: t); c.hour = h
                t = cal.date(from: c)!
                continue
            }
            let minute = comps.minute ?? 0
            if !minutes.contains(minute) {
                guard let mn = minutes.firstAtOrAfter(minute + 1) else {
                    t = cal.date(byAdding: .hour, value: 1, to: cal.startOfDay(for: t))!
                    continue
                }
                var c = cal.dateComponents([.year, .month, .day, .hour], from: t); c.minute = mn
                t = cal.date(from: c)!
                continue
            }
            let second = comps.second ?? 0
            if !seconds.contains(second) {
                guard let sc = seconds.firstAtOrAfter(second + 1) else {
                    t = cal.date(byAdding: .minute, value: 1, to: cal.startOfDay(for: t))!
                    continue
                }
                var c = cal.dateComponents([.year, .month, .day, .hour, .minute], from: t); c.second = sc
                t = cal.date(from: c)!
                continue
            }
            _ = comps
            return t
        }
        return nil
    }
}

/// Minimal cron runner: checks each registered schedule once per second.
final class CronRunner: @unchecked Sendable {
    struct Entry {
        let id: String
        let schedule: CronSchedule
        let handler: () -> Void
        var nextFire: Date
    }

    private let lock = NSLock()
    private var entries: [String: Entry] = [:]
    private var timer: DispatchSourceTimer?
    private let queue = DispatchQueue(label: "gn-drive.cron")

    func start() {
        lock.lock()
        guard timer == nil else { lock.unlock(); return }
        let t = DispatchSource.makeTimerSource(queue: queue)
        t.schedule(deadline: .now() + 1, repeating: 1.0)
        t.setEventHandler { [weak self] in self?.tick() }
        t.resume()
        timer = t
        lock.unlock()
    }

    func stop() {
        lock.lock()
        timer?.cancel()
        timer = nil
        entries.removeAll()
        lock.unlock()
    }

    /// Returns false when the expression is invalid.
    @discardableResult
    func add(id: String, schedule: CronSchedule, handler: @escaping () -> Void) -> Bool {
        lock.lock(); defer { lock.unlock() }
        guard let next = schedule.next(after: Date()) else { return false }
        entries[id] = Entry(id: id, schedule: schedule, handler: handler, nextFire: next)
        return true
    }

    func remove(id: String) {
        lock.lock(); entries.removeValue(forKey: id); lock.unlock()
    }

    private func tick() {
        let now = Date()
        var fired: [Entry] = []
        lock.lock()
        for (id, var e) in entries {
            if e.nextFire <= now {
                fired.append(e)
                e.nextFire = e.schedule.next(after: now) ?? .distantFuture
                entries[id] = e
            }
        }
        lock.unlock()
        for e in fired { e.handler() }
    }
}
