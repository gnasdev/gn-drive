// service.health file writer — port of internal/service/health.go.
import Foundation

public struct ServiceHealth: Codable, Sendable {
    public var pid: Int = 0
    public var serviceName: String = "gn-drive"
    public var mode: String = "service"
    public var startedAt: Date = Date()
    public var lastHeartbeat: Date = Date()
    public var webPort: Int = 0
    public var lastSyncAt: Date? = nil
    public var nextScheduleAt: Date? = nil
    public var lastError: String = ""
    public var activeTasks: [String] = []

    enum CodingKeys: String, CodingKey {
        case pid, mode
        case serviceName = "service_name"
        case startedAt = "started_at"
        case lastHeartbeat = "last_heartbeat"
        case webPort = "web_port"
        case lastSyncAt = "last_sync_at"
        case nextScheduleAt = "next_schedule_at"
        case lastError = "last_error"
        case activeTasks = "active_tasks"
    }

    public static func healthPath(configDir: String) -> String {
        (configDir as NSString).appendingPathComponent("service.health")
    }

    /// Read the health file, if present.
    public static func read(configDir: String) -> ServiceHealth? {
        let p = healthPath(configDir: configDir)
        guard let data = try? Data(contentsOf: URL(fileURLWithPath: p)) else { return nil }
        let dec = JSONDecoder()
        dec.dateDecodingStrategy = .iso8601
        return try? dec.decode(ServiceHealth.self, from: data)
    }

    public var isStale: Bool {
        Date().timeIntervalSince(lastHeartbeat) > 60
    }
    public var uptime: TimeInterval {
        Date().timeIntervalSince(startedAt)
    }
}

/// Periodically writes service health JSON to disk.
public final class HealthWriter: @unchecked Sendable {
    private let path: String
    private var health: ServiceHealth
    private var timer: DispatchSourceTimer?
    private let queue = DispatchQueue(label: "gn-drive.health")
    private let lock = NSLock()
    private let period: TimeInterval

    public init(configDir: String, period: TimeInterval = 5) {
        self.path = ServiceHealth.healthPath(configDir: configDir)
        self.period = period > 0 ? period : 5
        self.health = ServiceHealth(pid: Int(getpid()))
    }

    public func start() throws {
        lock.lock()
        health.startedAt = Date()
        health.lastHeartbeat = Date()
        lock.unlock()
        try writeNow()
        let t = DispatchSource.makeTimerSource(queue: queue)
        t.schedule(deadline: .now() + period, repeating: period)
        t.setEventHandler { [weak self] in try? self?.writeNow() }
        t.resume()
        lock.lock(); timer = t; lock.unlock()
    }

    public func stop() {
        lock.lock()
        timer?.cancel()
        timer = nil
        lock.unlock()
    }

    public func setWebPort(_ port: Int) {
        lock.lock(); health.webPort = port; lock.unlock()
    }
    public func setActiveTasks(_ tasks: [String]) {
        lock.lock(); health.activeTasks = tasks; lock.unlock()
    }
    public func setLastError(_ e: String) {
        lock.lock(); health.lastError = e; lock.unlock()
    }
    public func markSync() {
        lock.lock(); health.lastSyncAt = Date(); lock.unlock()
    }
    public func markNextSchedule(_ d: Date) {
        lock.lock(); health.nextScheduleAt = d; lock.unlock()
    }

    private func writeNow() throws {
        lock.lock()
        health.lastHeartbeat = Date()
        let h = health
        lock.unlock()
        let enc = JSONEncoder()
        enc.dateEncodingStrategy = .iso8601
        let data = try enc.encode(h)
        try data.write(to: URL(fileURLWithPath: path), options: .atomic)
    }
}
