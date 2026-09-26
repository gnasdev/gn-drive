// Operation sync_config editor — full port of OperationSettingsPanel.vue +
// lib/syncConfig.ts (all ~40 rclone option fields, grouped sections).
// Serializes BOTH snake_case and camelCase keys, matching the web UI.
import SwiftUI
import GNDriveCore

/// Typed view of Operation.syncConfig — port of lib/syncConfig.ts SyncConfig.
struct SyncConfigModel {
    var action = "push"
    // Performance
    var parallel: Int? = nil
    var bandwidth: Int? = nil           // MB/s
    var multiThreadStreams: Int? = nil
    var bufferSize: String? = nil
    var retries: Int? = nil
    var lowLevelRetries: Int? = nil
    var maxDuration: String? = nil
    var checkFirst: Bool? = nil
    var orderBy: String? = nil
    var retriesSleep: String? = nil
    var tpsLimit: Double? = nil
    var connTimeout: String? = nil
    var ioTimeout: String? = nil
    // Filtering
    var includedPaths: [String]? = nil
    var excludedPaths: [String]? = nil
    var minSize: String? = nil
    var maxSize: String? = nil
    var maxAge: String? = nil
    var minAge: String? = nil
    var maxDepth: Int? = nil
    var filterFromFile: String? = nil
    var excludeIfPresent: String? = nil
    var useRegex: Bool? = nil
    var deleteExcluded: Bool? = nil
    // Safety
    var dryRun: Bool? = nil
    var maxDelete: Int? = nil
    var immutable: Bool? = nil
    var maxTransfer: String? = nil
    var maxDeleteSize: String? = nil
    var suffix: String? = nil
    var suffixKeepExtension: Bool? = nil
    var backupPath: String? = nil
    // Comparison
    var sizeOnly: Bool? = nil
    var updateMode: Bool? = nil
    var ignoreExisting: Bool? = nil
    // Sync-specific
    var deleteTiming: String? = nil
    // Bisync
    var conflictResolution: String? = nil
    var resilient: Bool? = nil
    var maxLock: String? = nil
    var checkAccess: Bool? = nil
    var conflictLoser: String? = nil
    var conflictSuffix: String? = nil

    // MARK: - parse (accepts snake_case or camelCase)

    static func parse(_ raw: [String: Any], fallbackAction: String = "push") -> SyncConfigModel {
        func n(_ keys: String...) -> Int? {
            for k in keys {
                if let v = raw[k] as? Int { return v }
                if let v = raw[k] as? NSNumber { return v.intValue }
                if let v = raw[k] as? String, !v.isEmpty, let i = Int(v) { return i }
            }
            return nil
        }
        func d(_ keys: String...) -> Double? {
            for k in keys {
                if let v = raw[k] as? Double { return v }
                if let v = raw[k] as? NSNumber { return v.doubleValue }
                if let v = raw[k] as? String, !v.isEmpty, let x = Double(v) { return x }
            }
            return nil
        }
        func s(_ keys: String...) -> String? {
            for k in keys {
                if let v = raw[k] as? String, !v.trimmingCharacters(in: .whitespaces).isEmpty { return v }
            }
            return nil
        }
        func b(_ keys: String...) -> Bool? {
            for k in keys { if let v = raw[k] as? Bool { return v } }
            return nil
        }
        func a(_ keys: String...) -> [String]? {
            for k in keys { if let v = raw[k] as? [String] { return v } }
            return nil
        }
        var c = SyncConfigModel()
        c.action = FlowAction.normalize(s("action") ?? fallbackAction)
        c.parallel = n("parallel")
        c.bandwidth = n("bandwidth")
        c.multiThreadStreams = n("multi_thread_streams", "multiThreadStreams")
        c.bufferSize = s("buffer_size", "bufferSize")
        c.retries = n("retries")
        c.lowLevelRetries = n("low_level_retries", "lowLevelRetries")
        c.maxDuration = s("max_duration", "maxDuration")
        c.checkFirst = b("check_first", "checkFirst")
        c.orderBy = s("order_by", "orderBy")
        c.retriesSleep = s("retries_sleep", "retriesSleep")
        c.tpsLimit = d("tps_limit", "tpsLimit")
        c.connTimeout = s("conn_timeout", "connTimeout")
        c.ioTimeout = s("io_timeout", "ioTimeout")
        c.includedPaths = a("included_paths", "includedPaths")
        c.excludedPaths = a("excluded_paths", "excludedPaths")
        c.minSize = s("min_size", "minSize")
        c.maxSize = s("max_size", "maxSize")
        c.maxAge = s("max_age", "maxAge")
        c.minAge = s("min_age", "minAge")
        c.maxDepth = n("max_depth", "maxDepth")
        c.filterFromFile = s("filter_from_file", "filterFromFile")
        c.excludeIfPresent = s("exclude_if_present", "excludeIfPresent")
        c.useRegex = b("use_regex", "useRegex")
        c.deleteExcluded = b("delete_excluded", "deleteExcluded")
        c.dryRun = b("dry_run", "dryRun")
        c.maxDelete = n("max_delete", "maxDelete")
        c.immutable = b("immutable")
        c.maxTransfer = s("max_transfer", "maxTransfer")
        c.maxDeleteSize = s("max_delete_size", "maxDeleteSize")
        c.suffix = s("suffix")
        c.suffixKeepExtension = b("suffix_keep_extension", "suffixKeepExtension")
        c.backupPath = s("backup_path", "backupPath")
        c.sizeOnly = b("size_only", "sizeOnly")
        c.updateMode = b("update_mode", "updateMode")
        c.ignoreExisting = b("ignore_existing", "ignoreExisting")
        c.deleteTiming = s("delete_timing", "deleteTiming")
        c.conflictResolution = s("conflict_resolution", "conflictResolution")
        c.resilient = b("resilient")
        c.maxLock = s("max_lock", "maxLock")
        c.checkAccess = b("check_access", "checkAccess")
        c.conflictLoser = s("conflict_loser", "conflictLoser")
        c.conflictSuffix = s("conflict_suffix", "conflictSuffix")
        return c
    }

    // MARK: - serialize (writes snake_case + camelCase like the web UI)

    func serialize() -> [String: Any] {
        var out: [String: Any] = ["action": action]
        func set(_ snake: String, _ camel: String, _ v: Any?) {
            guard let v else { return }
            if let s = v as? String, s.isEmpty { return }
            if let a = v as? [String], a.isEmpty { return }
            out[snake] = v
            out[camel] = v
        }
        set("parallel", "parallel", parallel)
        set("bandwidth", "bandwidth", bandwidth)
        set("multi_thread_streams", "multiThreadStreams", multiThreadStreams)
        set("buffer_size", "bufferSize", bufferSize)
        set("retries", "retries", retries)
        set("low_level_retries", "lowLevelRetries", lowLevelRetries)
        set("max_duration", "maxDuration", maxDuration)
        set("check_first", "checkFirst", checkFirst)
        set("order_by", "orderBy", orderBy)
        set("retries_sleep", "retriesSleep", retriesSleep)
        set("tps_limit", "tpsLimit", tpsLimit)
        set("conn_timeout", "connTimeout", connTimeout)
        set("io_timeout", "ioTimeout", ioTimeout)
        set("included_paths", "includedPaths", includedPaths)
        set("excluded_paths", "excludedPaths", excludedPaths)
        set("min_size", "minSize", minSize)
        set("max_size", "maxSize", maxSize)
        set("max_age", "maxAge", maxAge)
        set("min_age", "minAge", minAge)
        set("max_depth", "maxDepth", maxDepth)
        set("filter_from_file", "filterFromFile", filterFromFile)
        set("exclude_if_present", "excludeIfPresent", excludeIfPresent)
        set("use_regex", "useRegex", useRegex)
        set("delete_excluded", "deleteExcluded", deleteExcluded)
        set("dry_run", "dryRun", dryRun)
        set("max_delete", "maxDelete", maxDelete)
        set("immutable", "immutable", immutable)
        set("max_transfer", "maxTransfer", maxTransfer)
        set("max_delete_size", "maxDeleteSize", maxDeleteSize)
        set("suffix", "suffix", suffix)
        set("suffix_keep_extension", "suffixKeepExtension", suffixKeepExtension)
        set("backup_path", "backupPath", backupPath)
        set("size_only", "sizeOnly", sizeOnly)
        set("update_mode", "updateMode", updateMode)
        set("ignore_existing", "ignoreExisting", ignoreExisting)
        set("delete_timing", "deleteTiming", deleteTiming)
        set("conflict_resolution", "conflictResolution", conflictResolution)
        set("resilient", "resilient", resilient)
        set("max_lock", "maxLock", maxLock)
        set("check_access", "checkAccess", checkAccess)
        set("conflict_loser", "conflictLoser", conflictLoser)
        set("conflict_suffix", "conflictSuffix", conflictSuffix)
        return out
    }

    /// Chips summarizing non-default options (view mode) — port of
    /// syncConfigSummaryChips.
    var summaryChips: [String] {
        var chips: [String] = []
        if dryRun == true { chips.append("dry-run") }
        if let p = parallel, p > 0 { chips.append("×\(p)") }
        if let b = bandwidth, b > 0 { chips.append("\(b)M") }
        if let i = includedPaths, !i.isEmpty { chips.append("+\(i.count) include") }
        if let e = excludedPaths, !e.isEmpty { chips.append("−\(e.count) exclude") }
        if let m = maxAge { chips.append("max-age \(m)") }
        if minSize != nil || maxSize != nil { chips.append("size filter") }
        if let c = conflictResolution { chips.append("conflict:\(c)") }
        if immutable == true { chips.append("immutable") }
        if sizeOnly == true { chips.append("size-only") }
        return chips
    }
}

/// Full operation options editor — port of OperationSettingsPanel.vue.
struct OperationOptionsPanel: View {
    @Binding var config: SyncConfigModel
    var disabled: Bool = false

    private var isPush: Bool { config.action == "push" || config.action == "pull" }
    private var isBi: Bool { config.action == "bi" || config.action == "bi-resync" }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            // Action + dry run
            Picker("Action", selection: $config.action) {
                Text("Push").tag("push")
                Text("Bi (two-way)").tag("bi")
                Text("Bi resync").tag("bi-resync")
            }
            .disabled(disabled)
            Toggle("Dry run", isOn: boolB(\.dryRun)).disabled(disabled)

            OptionSection("Performance") {
                NumRow("Parallel transfers", value: intB(\.parallel), hint: "8")
                NumRow("Bandwidth (MB/s)", value: intB(\.bandwidth), hint: "0")
                NumRow("Multi-thread streams", value: intB(\.multiThreadStreams), hint: "4")
                TextRow("Buffer size", text: strB(\.bufferSize), hint: "16M")
                NumRow("Retries", value: intB(\.retries), hint: "3")
                NumRow("Low-level retries", value: intB(\.lowLevelRetries), hint: "10")
                TextRow("Max duration", text: strB(\.maxDuration), hint: "24h")
                ToggleRow("Check first", value: boolB(\.checkFirst))
                TextRow("Order by", text: strB(\.orderBy), hint: "size,ascending")
                TextRow("Retries sleep", text: strB(\.retriesSleep), hint: "0s")
                NumRow("TPS limit", value: doubleB(\.tpsLimit), hint: "0")
                TextRow("Connect timeout", text: strB(\.connTimeout), hint: "60s")
                TextRow("I/O timeout", text: strB(\.ioTimeout), hint: "300s")
            }

            OptionSection("Filtering") {
                ListRow("Include paths", list: listB(\.includedPaths))
                ListRow("Exclude paths", list: listB(\.excludedPaths))
                TextRow("Min size", text: strB(\.minSize), hint: "1M")
                TextRow("Max size", text: strB(\.maxSize), hint: "1G")
                TextRow("Max age", text: strB(\.maxAge), hint: "24h")
                TextRow("Min age", text: strB(\.minAge), hint: "0s")
                NumRow("Max depth", value: intB(\.maxDepth), hint: "0")
                TextRow("Filter from file", text: strB(\.filterFromFile), hint: "/path/to/filters")
                TextRow("Exclude if present", text: strB(\.excludeIfPresent), hint: ".ignore")
                ToggleRow("Use regex", value: boolB(\.useRegex))
                ToggleRow("Delete excluded", value: boolB(\.deleteExcluded))
            }

            OptionSection("Safety") {
                NumRow("Max delete", value: intB(\.maxDelete), hint: "0")
                ToggleRow("Immutable", value: boolB(\.immutable))
                TextRow("Max transfer", text: strB(\.maxTransfer), hint: "100G")
                TextRow("Max delete size", text: strB(\.maxDeleteSize), hint: "1G")
                TextRow("Suffix", text: strB(\.suffix), hint: ".bak")
                ToggleRow("Suffix keep ext", value: boolB(\.suffixKeepExtension))
                TextRow("Backup path", text: strB(\.backupPath), hint: "remote:backup")
            }

            OptionSection("Comparison") {
                ToggleRow("Size only", value: boolB(\.sizeOnly))
                ToggleRow("Update mode", value: boolB(\.updateMode))
                ToggleRow("Ignore existing", value: boolB(\.ignoreExisting))
            }

            if isPush {
                OptionSection("Sync options") {
                    Picker("Delete timing", selection: .init(
                        get: { config.deleteTiming ?? "" },
                        set: { config.deleteTiming = $0.isEmpty ? nil : $0 })) {
                        Text("Default").tag("")
                        Text("Before").tag("before")
                        Text("During").tag("during")
                        Text("After").tag("after")
                    }
                    .disabled(disabled)
                }
            }

            if isBi {
                OptionSection("Bisync options") {
                    Picker("Conflict resolution", selection: .init(
                        get: { config.conflictResolution ?? "none" },
                        set: { config.conflictResolution = $0 })) {
                        Text("None").tag("none")
                        Text("Path1 wins").tag("path1")
                        Text("Path2 wins").tag("path2")
                        Text("Newer").tag("newer")
                        Text("Older").tag("older")
                        Text("Larger").tag("larger")
                        Text("Smaller").tag("smaller")
                    }
                    .disabled(disabled)
                    ToggleRow("Resilient", value: boolB(\.resilient))
                    TextRow("Max lock", text: strB(\.maxLock), hint: "2m")
                    ToggleRow("Check access", value: boolB(\.checkAccess))
                    TextRow("Conflict loser", text: strB(\.conflictLoser), hint: "num")
                    TextRow("Conflict suffix", text: strB(\.conflictSuffix), hint: ".conflict1")
                }
            }
        }
        .font(.caption)
    }

    // MARK: - bindings (Optional<T> → UI controls)

    private func strB(_ kp: WritableKeyPath<SyncConfigModel, String?>) -> Binding<String> {
        Binding(get: { config[keyPath: kp] ?? "" },
                set: { config[keyPath: kp] = $0.trimmingCharacters(in: .whitespaces).isEmpty ? nil : $0 })
    }
    private func intB(_ kp: WritableKeyPath<SyncConfigModel, Int?>) -> Binding<String> {
        Binding(get: { config[keyPath: kp].map(String.init) ?? "" },
                set: { config[keyPath: kp] = Int($0.trimmingCharacters(in: .whitespaces)) })
    }
    private func doubleB(_ kp: WritableKeyPath<SyncConfigModel, Double?>) -> Binding<String> {
        Binding(get: { config[keyPath: kp].map { "\($0)" } ?? "" },
                set: { config[keyPath: kp] = Double($0.trimmingCharacters(in: .whitespaces)) })
    }
    private func boolB(_ kp: WritableKeyPath<SyncConfigModel, Bool?>) -> Binding<Bool> {
        Binding(get: { config[keyPath: kp] ?? false },
                set: { config[keyPath: kp] = $0 })
    }
    private func listB(_ kp: WritableKeyPath<SyncConfigModel, [String]?>) -> Binding<String> {
        Binding(get: { (config[keyPath: kp] ?? []).joined(separator: "\n") },
                set: {
                    let items = $0.split(separator: "\n").map { String($0).trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
                    config[keyPath: kp] = items.isEmpty ? nil : items
                })
    }
}

private struct OptionSection<Content: View>: View {
    let title: String
    let content: Content
    init(_ title: String, @ViewBuilder content: () -> Content) {
        self.title = title; self.content = content()
    }
    var body: some View {
        DisclosureGroup(title) {
            VStack(alignment: .leading, spacing: 6) { content }
                .padding(.top, 6)
        }
        .padding(8)
        .background(Color.secondary.opacity(0.06), in: RoundedRectangle(cornerRadius: 8))
    }
}

private struct NumRow: View {
    let label: String
    @Binding var value: String
    var hint: String
    init(_ label: String, value: Binding<String>, hint: String = "") {
        self.label = label; _value = value; self.hint = hint
    }
    var body: some View {
        HStack {
            Text(label).frame(width: 130, alignment: .leading)
            TextField(hint, text: $value)
                .textFieldStyle(.roundedBorder)
                .font(.caption.monospaced())
        }
    }
}

private struct TextRow: View {
    let label: String
    @Binding var text: String
    var hint: String
    init(_ label: String, text: Binding<String>, hint: String = "") {
        self.label = label; _text = text; self.hint = hint
    }
    var body: some View {
        HStack {
            Text(label).frame(width: 130, alignment: .leading)
            TextField(hint, text: $text)
                .textFieldStyle(.roundedBorder)
                .font(.caption.monospaced())
        }
    }
}

private struct ToggleRow: View {
    let label: String
    @Binding var value: Bool
    init(_ label: String, value: Binding<Bool>) {
        self.label = label; _value = value
    }
    var body: some View {
        Toggle(label, isOn: $value)
    }
}

private struct ListRow: View {
    let label: String
    @Binding var list: String
    init(_ label: String, list: Binding<String>) {
        self.label = label; _list = list
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label)
            TextEditor(text: $list)
                .font(.caption.monospaced())
                .frame(height: 56)
                .overlay(RoundedRectangle(cornerRadius: 4).stroke(Color.secondary.opacity(0.3)))
        }
    }
}
