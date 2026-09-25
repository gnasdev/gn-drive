// SQLite persistence and repositories — port of internal/store/*.
// Same schema as the Go version so existing gn-drive.db files load unchanged.
import Foundation

public enum StoreError: Error {
    case notFound
    case invalid(String)
}

public final class Store: @unchecked Sendable {
    public let db: Database
    private let logger: Logger

    public init(path: String, logger: Logger) throws {
        self.db = try Database(path: path)
        self.logger = logger
        try migrate()
        logger.info("database opened", ("path", path))
    }

    public func close() { db.close() }

    // --- Schema ------------------------------------------------------------

    private static let schema = """
    CREATE TABLE IF NOT EXISTS settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL DEFAULT ''
    );
    CREATE TABLE IF NOT EXISTS profiles (
        name                 TEXT PRIMARY KEY,
        from_path            TEXT NOT NULL DEFAULT '',
        to_path              TEXT NOT NULL DEFAULT '',
        direction            TEXT NOT NULL DEFAULT '',
        included_paths       TEXT NOT NULL DEFAULT '[]',
        excluded_paths       TEXT NOT NULL DEFAULT '[]',
        bandwidth            INTEGER NOT NULL DEFAULT 0,
        parallel             INTEGER NOT NULL DEFAULT 0,
        backup_path          TEXT NOT NULL DEFAULT '',
        cache_path           TEXT NOT NULL DEFAULT '',
        min_size             TEXT NOT NULL DEFAULT '',
        max_size             TEXT NOT NULL DEFAULT '',
        filter_from_file     TEXT NOT NULL DEFAULT '',
        exclude_if_present   TEXT NOT NULL DEFAULT '',
        use_regex            INTEGER NOT NULL DEFAULT 0,
        max_delete           INTEGER,
        immutable            INTEGER NOT NULL DEFAULT 0,
        conflict_resolution  TEXT NOT NULL DEFAULT '',
        multi_thread_streams INTEGER,
        buffer_size          TEXT NOT NULL DEFAULT '',
        fast_list            INTEGER NOT NULL DEFAULT 0,
        retries              INTEGER,
        low_level_retries    INTEGER,
        max_duration         TEXT NOT NULL DEFAULT ''
    );
    CREATE TABLE IF NOT EXISTS schedules (
        id           TEXT PRIMARY KEY,
        profile_name TEXT NOT NULL,
        action       TEXT NOT NULL DEFAULT 'push',
        cron_expr    TEXT NOT NULL DEFAULT '',
        enabled      INTEGER NOT NULL DEFAULT 1,
        last_run     TEXT,
        next_run     TEXT,
        last_result  TEXT NOT NULL DEFAULT '',
        created_at   TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE TABLE IF NOT EXISTS history (
        id                TEXT PRIMARY KEY,
        profile_name      TEXT NOT NULL DEFAULT '',
        action            TEXT NOT NULL DEFAULT '',
        status            TEXT NOT NULL DEFAULT '',
        start_time        TEXT NOT NULL DEFAULT '',
        end_time          TEXT NOT NULL DEFAULT '',
        duration          TEXT NOT NULL DEFAULT '',
        files_transferred INTEGER NOT NULL DEFAULT 0,
        bytes_transferred INTEGER NOT NULL DEFAULT 0,
        errors            INTEGER NOT NULL DEFAULT 0,
        error_message     TEXT NOT NULL DEFAULT ''
    );
    CREATE INDEX IF NOT EXISTS idx_history_start_time ON history(start_time DESC);
    CREATE TABLE IF NOT EXISTS boards (
        id               TEXT PRIMARY KEY,
        name             TEXT NOT NULL DEFAULT '',
        created_at       TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
        schedule_enabled INTEGER NOT NULL DEFAULT 0,
        cron_expr        TEXT NOT NULL DEFAULT '',
        last_run         TEXT,
        next_run         TEXT,
        last_result      TEXT NOT NULL DEFAULT ''
    );
    CREATE TABLE IF NOT EXISTS board_nodes (
        id          TEXT NOT NULL,
        board_id    TEXT NOT NULL,
        remote_name TEXT NOT NULL DEFAULT '',
        path        TEXT NOT NULL DEFAULT '',
        label       TEXT NOT NULL DEFAULT '',
        x           REAL NOT NULL DEFAULT 0,
        y           REAL NOT NULL DEFAULT 0,
        PRIMARY KEY (board_id, id),
        FOREIGN KEY (board_id) REFERENCES boards(id) ON DELETE CASCADE
    );
    CREATE TABLE IF NOT EXISTS board_edges (
        id          TEXT NOT NULL,
        board_id    TEXT NOT NULL,
        source_id   TEXT NOT NULL,
        target_id   TEXT NOT NULL,
        action      TEXT NOT NULL DEFAULT 'push',
        sync_config TEXT NOT NULL DEFAULT '{}',
        PRIMARY KEY (board_id, id),
        FOREIGN KEY (board_id) REFERENCES boards(id) ON DELETE CASCADE
    );
    CREATE TABLE IF NOT EXISTS flows (
        id               TEXT PRIMARY KEY,
        name             TEXT NOT NULL DEFAULT '',
        is_collapsed     INTEGER NOT NULL DEFAULT 0,
        schedule_enabled INTEGER NOT NULL DEFAULT 0,
        cron_expr        TEXT NOT NULL DEFAULT '',
        sort_order       INTEGER NOT NULL DEFAULT 0,
        created_at       TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
        canvas_json      TEXT NOT NULL DEFAULT '{}'
    );
    CREATE INDEX IF NOT EXISTS idx_flows_sort_order ON flows(sort_order);
    CREATE TABLE IF NOT EXISTS operations (
        id            TEXT PRIMARY KEY,
        flow_id       TEXT NOT NULL,
        source_remote TEXT NOT NULL DEFAULT '',
        source_path   TEXT NOT NULL DEFAULT '/',
        target_remote TEXT NOT NULL DEFAULT '',
        target_path   TEXT NOT NULL DEFAULT '/',
        action        TEXT NOT NULL DEFAULT 'push',
        sync_config   TEXT NOT NULL DEFAULT '{}',
        is_expanded   INTEGER NOT NULL DEFAULT 0,
        sort_order    INTEGER NOT NULL DEFAULT 0,
        FOREIGN KEY (flow_id) REFERENCES flows(id) ON DELETE CASCADE
    );
    CREATE INDEX IF NOT EXISTS idx_operations_flow_id ON operations(flow_id);
    CREATE TABLE IF NOT EXISTS delta_state (
        remote_key     TEXT PRIMARY KEY,
        provider       TEXT NOT NULL DEFAULT '',
        is_watching    INTEGER NOT NULL DEFAULT 0,
        last_full_sync TEXT,
        delta_count    INTEGER NOT NULL DEFAULT 0,
        created_at     TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at     TEXT NOT NULL DEFAULT (datetime('now'))
    );
    """

    private func migrate() throws {
        // Execute each statement separately (sqlite3_exec handles all at once,
        // but exec() prepares a single statement — split on ";" boundaries).
        for stmt in Self.schema.components(separatedBy: ";\n") {
            let sql = stmt.trimmingCharacters(in: .whitespacesAndNewlines)
            if !sql.isEmpty { _ = try db.exec(sql) }
        }
        migrateProfilesNewColumns()
        try? db.exec("ALTER TABLE flows ADD COLUMN canvas_json TEXT NOT NULL DEFAULT '{}'")
        try applyMigrations()
    }

    private func migrateProfilesNewColumns() {
        for ddl in [
            "ALTER TABLE profiles ADD COLUMN max_age TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE profiles ADD COLUMN min_age TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE profiles ADD COLUMN max_depth INTEGER",
            "ALTER TABLE profiles ADD COLUMN delete_excluded INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE profiles ADD COLUMN dry_run INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE profiles ADD COLUMN max_transfer TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE profiles ADD COLUMN max_delete_size TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE profiles ADD COLUMN suffix TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE profiles ADD COLUMN suffix_keep_extension INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE profiles ADD COLUMN check_first INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE profiles ADD COLUMN order_by TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE profiles ADD COLUMN retries_sleep TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE profiles ADD COLUMN tps_limit REAL",
            "ALTER TABLE profiles ADD COLUMN conn_timeout TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE profiles ADD COLUMN io_timeout TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE profiles ADD COLUMN size_only INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE profiles ADD COLUMN update_mode INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE profiles ADD COLUMN ignore_existing INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE profiles ADD COLUMN delete_timing TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE profiles ADD COLUMN resilient INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE profiles ADD COLUMN max_lock TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE profiles ADD COLUMN check_access INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE profiles ADD COLUMN conflict_loser TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE profiles ADD COLUMN conflict_suffix TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE profiles ADD COLUMN direction TEXT NOT NULL DEFAULT ''",
        ] {
            _ = try? db.exec(ddl) // duplicate column → ignore
        }
    }

    private static let schemaVersion = 1

    private func applyMigrations() throws {
        let row = try db.queryRow("PRAGMA user_version")
        var current = Int(row?.first?.int ?? 0)
        let migrations: [(Int, String)] = [(1, "")]
        for (version, sql) in migrations where version > current {
            if !sql.isEmpty { _ = try db.exec(sql) }
            _ = try db.exec("PRAGMA user_version = \(version)")
            current = version
            logger.info("store: applied migration", ("version", String(version)))
        }
    }

    // --- Profiles ----------------------------------------------------------

    private static let profileColumns = "name, from_path, to_path, direction, included_paths, excluded_paths, bandwidth, parallel, backup_path, cache_path, min_size, max_size, filter_from_file, exclude_if_present, use_regex, max_delete, immutable, conflict_resolution, multi_thread_streams, buffer_size, fast_list, retries, low_level_retries, max_duration, max_age, min_age, max_depth, delete_excluded, dry_run, max_transfer, max_delete_size, suffix, suffix_keep_extension, check_first, order_by, retries_sleep, tps_limit, conn_timeout, io_timeout, size_only, update_mode, ignore_existing, delete_timing, resilient, max_lock, check_access, conflict_loser, conflict_suffix"

    public func listProfiles() throws -> [Profile] {
        let rows = try db.query("SELECT \(Self.profileColumns) FROM profiles ORDER BY name")
        return try rows.map { try Self.scanProfile($0) }
    }

    public func getProfile(_ name: String) throws -> Profile {
        guard let row = try db.queryRow("SELECT \(Self.profileColumns) FROM profiles WHERE name = ?", .text(name)) else {
            throw StoreError.notFound
        }
        return try Self.scanProfile(row)
    }

    public func saveProfile(_ p: inout Profile) throws {
        guard !p.name.isEmpty else { throw StoreError.invalid("profile: name is required") }
        var dir = p.direction.trimmingCharacters(in: .whitespaces)
        if dir.isEmpty { dir = ProfileDirection.push }
        guard ProfileDirection.isValid(dir) else {
            throw StoreError.invalid("profile: invalid direction \(p.direction)")
        }
        p.direction = dir
        let nullableInt: (Int?) -> SQLValue = { $0.map { .int(Int64($0)) } ?? .null }
        let nullableDouble: (Double?) -> SQLValue = { $0.map { .double($0) } ?? .null }
        _ = try db.exec("""
            INSERT INTO profiles (\(Self.profileColumns)) VALUES (
            ?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
            ON CONFLICT(name) DO UPDATE SET
              from_path=excluded.from_path, to_path=excluded.to_path, direction=excluded.direction,
              included_paths=excluded.included_paths, excluded_paths=excluded.excluded_paths,
              bandwidth=excluded.bandwidth, parallel=excluded.parallel, backup_path=excluded.backup_path,
              cache_path=excluded.cache_path, min_size=excluded.min_size, max_size=excluded.max_size,
              filter_from_file=excluded.filter_from_file, exclude_if_present=excluded.exclude_if_present,
              use_regex=excluded.use_regex, max_delete=excluded.max_delete, immutable=excluded.immutable,
              conflict_resolution=excluded.conflict_resolution,
              multi_thread_streams=excluded.multi_thread_streams, buffer_size=excluded.buffer_size,
              fast_list=excluded.fast_list, retries=excluded.retries,
              low_level_retries=excluded.low_level_retries, max_duration=excluded.max_duration,
              max_age=excluded.max_age, min_age=excluded.min_age, max_depth=excluded.max_depth,
              delete_excluded=excluded.delete_excluded, dry_run=excluded.dry_run,
              max_transfer=excluded.max_transfer, max_delete_size=excluded.max_delete_size,
              suffix=excluded.suffix, suffix_keep_extension=excluded.suffix_keep_extension,
              check_first=excluded.check_first, order_by=excluded.order_by,
              retries_sleep=excluded.retries_sleep, tps_limit=excluded.tps_limit,
              conn_timeout=excluded.conn_timeout, io_timeout=excluded.io_timeout,
              size_only=excluded.size_only, update_mode=excluded.update_mode,
              ignore_existing=excluded.ignore_existing, delete_timing=excluded.delete_timing,
              resilient=excluded.resilient, max_lock=excluded.max_lock,
              check_access=excluded.check_access, conflict_loser=excluded.conflict_loser,
              conflict_suffix=excluded.conflict_suffix
            """,
            .text(p.name), .text(p.from), .text(p.to), .text(p.direction),
            .text(jsonString(p.includedPaths)), .text(jsonString(p.excludedPaths)),
            .int(Int64(p.bandwidth)), .int(Int64(p.parallel)), .text(p.backupPath), .text(p.cachePath),
            .text(p.minSize), .text(p.maxSize), .text(p.filterFromFile), .text(p.excludeIfPresent),
            .int(p.useRegex ? 1 : 0), nullableInt(p.maxDelete), .int(p.immutable ? 1 : 0),
            .text(p.conflictResolution), nullableInt(p.multiThreadStreams),
            .text(p.bufferSize), .int(p.fastList ? 1 : 0),
            nullableInt(p.retries), nullableInt(p.lowLevelRetries), .text(p.maxDuration),
            .text(p.maxAge), .text(p.minAge), nullableInt(p.maxDepth), .int(p.deleteExcluded ? 1 : 0),
            .int(p.dryRun ? 1 : 0), .text(p.maxTransfer), .text(p.maxDeleteSize), .text(p.suffix),
            .int(p.suffixKeepExtension ? 1 : 0),
            .int(p.checkFirst ? 1 : 0), .text(p.orderBy), .text(p.retriesSleep), nullableDouble(p.tpsLimit),
            .text(p.connTimeout), .text(p.ioTimeout), .int(p.sizeOnly ? 1 : 0), .int(p.updateMode ? 1 : 0),
            .int(p.ignoreExisting ? 1 : 0), .text(p.deleteTiming), .int(p.resilient ? 1 : 0),
            .text(p.maxLock), .int(p.checkAccess ? 1 : 0), .text(p.conflictLoser), .text(p.conflictSuffix))
    }

    public func deleteProfile(_ name: String) throws {
        let n = try db.exec("DELETE FROM profiles WHERE name = ?", .text(name))
        if n == 0 { throw StoreError.notFound }
    }

    private static func scanProfile(_ row: [SQLValue]) throws -> Profile {
        var p = Profile()
        p.name = row[0].text
        p.from = row[1].text
        p.to = row[2].text
        p.direction = row[3].text
        p.includedPaths = stringSlice(row[4].text)
        p.excludedPaths = stringSlice(row[5].text)
        p.bandwidth = Int(row[6].int)
        p.parallel = Int(row[7].int)
        p.backupPath = row[8].text
        p.cachePath = row[9].text
        p.minSize = row[10].text
        p.maxSize = row[11].text
        p.filterFromFile = row[12].text
        p.excludeIfPresent = row[13].text
        p.useRegex = row[14].int != 0
        p.maxDelete = row[15].isNull ? nil : Int(row[15].int)
        p.immutable = row[16].int != 0
        p.conflictResolution = row[17].text
        p.multiThreadStreams = row[18].isNull ? nil : Int(row[18].int)
        p.bufferSize = row[19].text
        p.fastList = row[20].int != 0
        p.retries = row[21].isNull ? nil : Int(row[21].int)
        p.lowLevelRetries = row[22].isNull ? nil : Int(row[22].int)
        p.maxDuration = row[23].text
        p.maxAge = row[24].text
        p.minAge = row[25].text
        p.maxDepth = row[26].isNull ? nil : Int(row[26].int)
        p.deleteExcluded = row[27].int != 0
        p.dryRun = row[28].int != 0
        p.maxTransfer = row[29].text
        p.maxDeleteSize = row[30].text
        p.suffix = row[31].text
        p.suffixKeepExtension = row[32].int != 0
        p.checkFirst = row[33].int != 0
        p.orderBy = row[34].text
        p.retriesSleep = row[35].text
        p.tpsLimit = row[36].isNull ? nil : row[36].double
        p.connTimeout = row[37].text
        p.ioTimeout = row[38].text
        p.sizeOnly = row[39].int != 0
        p.updateMode = row[40].int != 0
        p.ignoreExisting = row[41].int != 0
        p.deleteTiming = row[42].text
        p.resilient = row[43].int != 0
        p.maxLock = row[44].text
        p.checkAccess = row[45].int != 0
        p.conflictLoser = row[46].text
        p.conflictSuffix = row[47].text
        return p
    }

    // --- Schedules ---------------------------------------------------------

    public func listSchedules() throws -> [Schedule] {
        let rows = try db.query("""
            SELECT id, profile_name, action, cron_expr, enabled, last_run, next_run, last_result, created_at
            FROM schedules ORDER BY created_at DESC
            """)
        return rows.map { row in
            var s = Schedule()
            s.id = row[0].text
            s.profileName = row[1].text
            s.action = row[2].text
            s.cron = row[3].text
            s.enabled = row[4].int != 0
            s.lastRun = row[5].text
            s.nextRun = row[6].text
            s.lastResult = row[7].text
            s.createdAt = row[8].text
            return s
        }
    }

    public func saveSchedule(_ s: Schedule) throws {
        guard !s.id.isEmpty else { throw StoreError.invalid("schedule: id is required") }
        _ = try db.exec("""
            INSERT INTO schedules (id, profile_name, action, cron_expr, enabled, last_run, next_run, last_result)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
              profile_name=excluded.profile_name, action=excluded.action,
              cron_expr=excluded.cron_expr, enabled=excluded.enabled,
              last_run=excluded.last_run, next_run=excluded.next_run,
              last_result=excluded.last_result
            """,
            .text(s.id), .text(s.profileName), .text(s.action), .text(s.cron), .int(s.enabled ? 1 : 0),
            s.lastRun.isEmpty ? .null : .text(s.lastRun),
            s.nextRun.isEmpty ? .null : .text(s.nextRun),
            .text(s.lastResult))
    }

    public func deleteSchedule(_ id: String) throws {
        let n = try db.exec("DELETE FROM schedules WHERE id = ?", .text(id))
        if n == 0 { throw StoreError.notFound }
    }

    // --- History -----------------------------------------------------------

    public func listHistory(limit: Int, offset: Int) throws -> [HistoryEntry] {
        let rows = try db.query("""
            SELECT id, profile_name, action, status, start_time, end_time, duration,
                   files_transferred, bytes_transferred, errors, error_message
            FROM history ORDER BY start_time DESC LIMIT ? OFFSET ?
            """, .int(Int64(limit)), .int(Int64(offset)))
        return rows.map(Self.scanHistory)
    }

    public func listHistory(profile: String, limit: Int, offset: Int) throws -> [HistoryEntry] {
        let rows = try db.query("""
            SELECT id, profile_name, action, status, start_time, end_time, duration,
                   files_transferred, bytes_transferred, errors, error_message
            FROM history WHERE profile_name = ? ORDER BY start_time DESC LIMIT ? OFFSET ?
            """, .text(profile), .int(Int64(limit)), .int(Int64(offset)))
        return rows.map(Self.scanHistory)
    }

    public func saveHistory(_ e: HistoryEntry) throws {
        guard !e.id.isEmpty else { throw StoreError.invalid("history: id is required") }
        _ = try db.exec("""
            INSERT INTO history (id, profile_name, action, status, start_time, end_time, duration,
                                 files_transferred, bytes_transferred, errors, error_message)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
              status=excluded.status, end_time=excluded.end_time, duration=excluded.duration,
              files_transferred=excluded.files_transferred, bytes_transferred=excluded.bytes_transferred,
              errors=excluded.errors, error_message=excluded.error_message
            """,
            .text(e.id), .text(e.profileName), .text(e.action), .text(e.state),
            .text(e.startedAt), .text(e.finishedAt), .text(String(e.duration)),
            .int(Int64(e.files)), .int(e.bytes), .int(Int64(e.errors)), .text(e.errorMessage))
    }

    public func clearHistory() throws {
        _ = try db.exec("DELETE FROM history")
    }

    public func historyStats() throws -> HistoryStats {
        var stats = HistoryStats()
        if let row = try db.queryRow("""
            SELECT COUNT(*), COALESCE(SUM(bytes_transferred),0), COALESCE(SUM(CAST(duration AS INTEGER)),0), COALESCE(SUM(errors),0)
            FROM history
            """) {
            stats.totalSyncs = Int(row[0].int)
            stats.totalBytes = row[1].int
            stats.totalDuration = row[2].int
            stats.totalErrors = Int(row[3].int)
        }
        let rows = try db.query("""
            SELECT profile_name, COUNT(*), COALESCE(SUM(bytes_transferred),0),
                   COALESCE(SUM(CAST(duration AS INTEGER)),0), COALESCE(SUM(errors),0)
            FROM history GROUP BY profile_name
            """)
        for row in rows {
            stats.byProfile[row[0].text] = ProfileStats(syncs: Int(row[1].int), bytes: row[2].int,
                                                        duration: row[3].int, errors: Int(row[4].int))
        }
        return stats
    }

    private static func scanHistory(_ row: [SQLValue]) -> HistoryEntry {
        var e = HistoryEntry()
        e.id = row[0].text
        e.profileName = row[1].text
        e.action = row[2].text
        e.state = row[3].text
        e.startedAt = row[4].text
        e.finishedAt = row[5].text
        e.duration = Int64(row[6].text) ?? row[6].int
        e.files = Int(row[7].int)
        e.bytes = row[8].int
        e.errors = Int(row[9].int)
        e.errorMessage = row[10].text
        return e
    }

    // --- Settings ----------------------------------------------------------

    public func getSetting(_ key: String) throws -> String {
        guard let row = try db.queryRow("SELECT value FROM settings WHERE key = ?", .text(key)) else {
            throw StoreError.notFound
        }
        return row[0].text
    }

    public func setSetting(_ key: String, _ value: String) throws {
        _ = try db.exec("""
            INSERT INTO settings (key, value) VALUES (?, ?)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            """, .text(key), .text(value))
    }

    public func getSettingBool(_ key: String, default def: Bool) -> Bool {
        guard let v = try? getSetting(key) else { return def }
        return v == "true" || v == "1"
    }

    // --- Boards ------------------------------------------------------------

    public func listBoards() throws -> [Board] {
        let rows = try db.query("""
            SELECT id, name, created_at, updated_at, schedule_enabled, cron_expr
            FROM boards ORDER BY updated_at DESC
            """)
        return rows.map { row in
            var b = Board()
            b.id = row[0].text
            b.name = row[1].text
            b.createdAt = row[2].text
            b.updatedAt = row[3].text
            return b
        }
    }

    public func getBoard(_ id: String) throws -> Board {
        guard let row = try db.queryRow("""
            SELECT id, name, created_at, updated_at, schedule_enabled, cron_expr
            FROM boards WHERE id = ?
            """, .text(id)) else { throw StoreError.notFound }
        var b = Board()
        b.id = row[0].text
        b.name = row[1].text
        b.createdAt = row[2].text
        b.updatedAt = row[3].text
        return b
    }

    public func loadBoardGraph(_ id: String) throws -> Board {
        var b = try getBoard(id)
        let nrows = try db.query("""
            SELECT id, remote_name, path, label, x, y FROM board_nodes WHERE board_id = ? ORDER BY id
            """, .text(id))
        b.nodes = nrows.map { row in
            var n = BoardNode()
            n.id = row[0].text
            n.remoteName = row[1].text
            n.path = row[2].text
            n.label = row[3].text
            n.x = row[4].double
            n.y = row[5].double
            return n
        }
        let erows = try db.query("""
            SELECT id, source_id, target_id, action, sync_config FROM board_edges WHERE board_id = ? ORDER BY id
            """, .text(id))
        b.edges = erows.map { row in
            var e = BoardEdge()
            e.id = row[0].text
            e.sourceID = row[1].text
            e.targetID = row[2].text
            e.action = row[3].text
            e.syncConfig = row[4].text.isEmpty ? "{}" : row[4].text
            return e
        }
        return b
    }

    public func saveBoardGraph(_ b: Board) throws {
        try db.transaction {
            _ = try db.execInTransaction("""
                INSERT INTO boards (id, name, schedule_enabled, cron_expr, updated_at)
                VALUES (?, ?, ?, ?, datetime('now'))
                ON CONFLICT(id) DO UPDATE SET
                  name=excluded.name, schedule_enabled=excluded.schedule_enabled,
                  cron_expr=excluded.cron_expr, updated_at=datetime('now')
                """, .text(b.id), .text(b.name), .int(0), .text(""))
            _ = try db.execInTransaction("DELETE FROM board_nodes WHERE board_id = ?", .text(b.id))
            for n in b.nodes {
                _ = try db.execInTransaction("""
                    INSERT INTO board_nodes (id, board_id, remote_name, path, label, x, y)
                    VALUES (?, ?, ?, ?, ?, ?, ?)
                    """, .text(n.id), .text(b.id), .text(n.remoteName), .text(n.path),
                    .text(n.label), .double(n.x), .double(n.y))
            }
            _ = try db.execInTransaction("DELETE FROM board_edges WHERE board_id = ?", .text(b.id))
            for e in b.edges {
                _ = try db.execInTransaction("""
                    INSERT INTO board_edges (id, board_id, source_id, target_id, action, sync_config)
                    VALUES (?, ?, ?, ?, ?, ?)
                    """, .text(e.id), .text(b.id), .text(e.sourceID), .text(e.targetID),
                    .text(e.action), .text(e.syncConfig.isEmpty ? "{}" : e.syncConfig))
            }
        }
    }

    public func deleteBoard(_ id: String) throws {
        let n = try db.exec("DELETE FROM boards WHERE id = ?", .text(id))
        if n == 0 { throw StoreError.notFound }
    }

    // --- Flows -------------------------------------------------------------

    public func listFlows() throws -> [Flow] {
        let rows = try db.query("""
            SELECT id, name, is_collapsed, schedule_enabled, cron_expr, sort_order, created_at, updated_at, canvas_json
            FROM flows ORDER BY sort_order, name
            """)
        var flows = try rows.map { try Self.scanFlow($0) }
        for i in flows.indices {
            flows[i].operations = try listOperations(flowID: flows[i].id)
        }
        return flows
    }

    public func getFlow(_ id: String) throws -> Flow {
        guard let row = try db.queryRow("""
            SELECT id, name, is_collapsed, schedule_enabled, cron_expr, sort_order, created_at, updated_at, canvas_json
            FROM flows WHERE id = ?
            """, .text(id)) else { throw StoreError.notFound }
        var f = try Self.scanFlow(row)
        f.operations = try listOperations(flowID: f.id)
        return f
    }

    private static func scanFlow(_ row: [SQLValue]) throws -> Flow {
        var f = Flow()
        f.id = row[0].text
        f.name = row[1].text
        f.isCollapsed = row[2].int != 0
        f.scheduleEnabled = row[3].int != 0
        f.scheduleCron = row[4].isNull ? "" : row[4].text
        f.sortOrder = Int(row[5].int)
        f.createdAt = row[6].isNull ? "" : row[6].text
        f.updatedAt = row[7].isNull ? "" : row[7].text
        f.canvasJSON = row[8].isNull || row[8].text.isEmpty ? "{}" : row[8].text
        return f
    }

    public func listOperations(flowID: String) throws -> [FlowOperation] {
        let rows = try db.query("""
            SELECT id, flow_id, source_remote, source_path, target_remote, target_path,
                   action, sync_config, is_expanded, sort_order
            FROM operations WHERE flow_id = ? ORDER BY sort_order, id
            """, .text(flowID))
        return rows.map { row in
            var op = FlowOperation()
            op.id = row[0].text
            op.flowID = row[1].text
            op.sourceRemote = row[2].text
            op.sourcePath = row[3].text
            op.targetRemote = row[4].text
            op.targetPath = row[5].text
            op.action = row[6].text
            let cfg = row[7].text
            if !cfg.isEmpty, let data = cfg.data(using: .utf8),
               let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
                op.syncConfig = JSONDict(obj)
            }
            op.isExpanded = row[8].int != 0
            op.sortOrder = Int(row[9].int)
            op.normalizeAction()
            return op
        }
    }

    /// Upsert a flow and replace its operations (Wails SaveFlows semantics).
    public func saveFlow(_ f: inout Flow) throws {
        guard !f.id.isEmpty else { throw StoreError.invalid("flow: id is required") }
        let schedEnabled = f.scheduleEnabled
        f.scheduleEnabled = schedEnabled
        let cron = f.scheduleCron

        try db.transaction {
            _ = try db.execInTransaction("""
                INSERT INTO flows (id, name, is_collapsed, schedule_enabled, cron_expr, sort_order, created_at, updated_at, canvas_json)
                VALUES (?, ?, ?, ?, ?, ?, COALESCE(NULLIF(?, ''), datetime('now')), datetime('now'), ?)
                ON CONFLICT(id) DO UPDATE SET
                  name=excluded.name, is_collapsed=excluded.is_collapsed,
                  schedule_enabled=excluded.schedule_enabled, cron_expr=excluded.cron_expr,
                  sort_order=excluded.sort_order, updated_at=datetime('now'),
                  canvas_json=excluded.canvas_json
                """,
                .text(f.id), .text(f.name), .int(f.isCollapsed ? 1 : 0), .int(schedEnabled ? 1 : 0),
                .text(cron), .int(Int64(f.sortOrder)), .text(f.createdAt),
                .text(f.canvasJSON.isEmpty ? "{}" : f.canvasJSON))

            _ = try db.execInTransaction("DELETE FROM operations WHERE flow_id = ?", .text(f.id))
            for i in f.operations.indices {
                var op = f.operations[i]
                guard !op.id.isEmpty else {
                    throw StoreError.invalid("flow: operation id is required")
                }
                op.flowID = f.id
                if op.sortOrder == 0 { op.sortOrder = i }
                op.normalizeAction()
                let cfgData = try? JSONSerialization.data(withJSONObject: op.syncConfig.value)
                let cfg = cfgData.flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
                _ = try db.execInTransaction("""
                    INSERT INTO operations
                    (id, flow_id, source_remote, source_path, target_remote, target_path, action, sync_config, is_expanded, sort_order)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    """,
                    .text(op.id), .text(f.id), .text(op.sourceRemote),
                    .text(op.sourcePath.isEmpty ? "/" : op.sourcePath),
                    .text(op.targetRemote),
                    .text(op.targetPath.isEmpty ? "/" : op.targetPath),
                    .text(op.action), .text(cfg), .int(op.isExpanded ? 1 : 0), .int(Int64(op.sortOrder)))
            }
        }
    }

    public func deleteFlow(_ id: String) throws {
        let n = try db.exec("DELETE FROM flows WHERE id = ?", .text(id))
        if n == 0 { throw StoreError.notFound }
    }

    // --- Delta state -------------------------------------------------------

    public func getDeltaState(_ remoteKey: String) throws -> DeltaState {
        guard let row = try db.queryRow("""
            SELECT remote_key, provider, last_full_sync, delta_count, is_watching
            FROM delta_state WHERE remote_key = ?
            """, .text(remoteKey)) else { throw StoreError.notFound }
        var d = DeltaState()
        d.remoteKey = row[0].text
        d.provider = row[1].text
        d.lastFullSync = row[2].isNull ? "" : row[2].text
        d.deltaCount = Int(row[3].int)
        d.isWatching = row[4].int != 0
        return d
    }

    public func recordFullSync(remoteKey: String, provider: String) throws {
        let now = AuthService.rfc3339(Date())
        _ = try db.exec("""
            INSERT INTO delta_state (remote_key, provider, last_full_sync, delta_count, is_watching)
            VALUES (?, ?, ?, 0, 0)
            ON CONFLICT(remote_key) DO UPDATE SET
              last_full_sync=excluded.last_full_sync, delta_count=0, is_watching=0
            """, .text(remoteKey), .text(provider), .text(now))
    }
}

// --- JSON helpers ------------------------------------------------------------

func jsonString(_ arr: [String]) -> String {
    guard let d = try? JSONSerialization.data(withJSONObject: arr) else { return "[]" }
    return String(data: d, encoding: .utf8) ?? "[]"
}

func stringSlice(_ s: String) -> [String] {
    guard !s.isEmpty, let d = s.data(using: .utf8),
          let arr = try? JSONSerialization.jsonObject(with: d) as? [String] else { return [] }
    return arr
}
