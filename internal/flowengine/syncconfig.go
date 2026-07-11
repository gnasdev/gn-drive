package flowengine

import (
	"encoding/json"
	"strconv"
	"strings"

	"github.com/gnasdev/gn-drive/internal/store"
)

// profileFromSyncConfig maps a flow Operation.sync_config JSON bag onto a
// store.Profile used as the flag carrier for StartPathSync (Wails SyncConfig).
func profileFromSyncConfig(raw json.RawMessage) *store.Profile {
	p := &store.Profile{Parallel: 4}
	if len(raw) == 0 {
		return p
	}
	var m map[string]any
	if err := json.Unmarshal(raw, &m); err != nil {
		return p
	}

	if v, ok := asBool(m, "dry_run", "dryRun"); ok {
		p.DryRun = v
	}
	if v, ok := asInt(m, "parallel"); ok && v > 0 {
		p.Parallel = v
	}
	if v, ok := asInt(m, "bandwidth"); ok && v > 0 {
		p.Bandwidth = v
	}
	if v, ok := asInt(m, "multi_thread_streams", "multiThreadStreams"); ok && v > 0 {
		p.MultiThreadStreams = intPtr(v)
	}
	if v, ok := asString(m, "buffer_size", "bufferSize"); ok {
		p.BufferSize = v
	}
	if v, ok := asInt(m, "retries"); ok && v > 0 {
		p.Retries = intPtr(v)
	}
	if v, ok := asInt(m, "low_level_retries", "lowLevelRetries"); ok && v > 0 {
		p.LowLevelRetries = intPtr(v)
	}
	if v, ok := asString(m, "max_duration", "maxDuration"); ok {
		p.MaxDuration = v
	}
	if v, ok := asBool(m, "check_first", "checkFirst"); ok {
		p.CheckFirst = v
	}
	if v, ok := asString(m, "order_by", "orderBy"); ok {
		p.OrderBy = v
	}
	if v, ok := asString(m, "retries_sleep", "retriesSleep"); ok {
		p.RetriesSleep = v
	}
	if v, ok := asFloat(m, "tps_limit", "tpsLimit"); ok && v > 0 {
		p.TpsLimit = floatPtr(v)
	}
	if v, ok := asString(m, "conn_timeout", "connTimeout"); ok {
		p.ConnTimeout = v
	}
	if v, ok := asString(m, "io_timeout", "ioTimeout"); ok {
		p.IoTimeout = v
	}

	if v, ok := asStringSlice(m, "included_paths", "includedPaths"); ok {
		p.IncludedPaths = v
	}
	if v, ok := asStringSlice(m, "excluded_paths", "excludedPaths"); ok {
		p.ExcludedPaths = v
	}
	if v, ok := asString(m, "min_size", "minSize"); ok {
		p.MinSize = v
	}
	if v, ok := asString(m, "max_size", "maxSize"); ok {
		p.MaxSize = v
	}
	if v, ok := asString(m, "max_age", "maxAge"); ok {
		p.MaxAge = v
	}
	if v, ok := asString(m, "min_age", "minAge"); ok {
		p.MinAge = v
	}
	if v, ok := asInt(m, "max_depth", "maxDepth"); ok && v > 0 {
		p.MaxDepth = intPtr(v)
	}
	if v, ok := asString(m, "filter_from_file", "filterFromFile"); ok {
		p.FilterFromFile = v
	}
	if v, ok := asString(m, "exclude_if_present", "excludeIfPresent"); ok {
		p.ExcludeIfPresent = v
	}
	if v, ok := asBool(m, "use_regex", "useRegex"); ok {
		p.UseRegex = v
	}
	if v, ok := asBool(m, "delete_excluded", "deleteExcluded"); ok {
		p.DeleteExcluded = v
	}

	if v, ok := asInt(m, "max_delete", "maxDelete"); ok && v > 0 {
		p.MaxDelete = intPtr(v)
	}
	if v, ok := asBool(m, "immutable"); ok {
		p.Immutable = v
	}
	if v, ok := asString(m, "max_transfer", "maxTransfer"); ok {
		p.MaxTransfer = v
	}
	if v, ok := asString(m, "max_delete_size", "maxDeleteSize"); ok {
		p.MaxDeleteSize = v
	}
	if v, ok := asString(m, "suffix"); ok {
		p.Suffix = v
	}
	if v, ok := asBool(m, "suffix_keep_extension", "suffixKeepExtension"); ok {
		p.SuffixKeepExtension = v
	}
	if v, ok := asString(m, "backup_path", "backupPath"); ok {
		p.BackupPath = v
	}

	if v, ok := asBool(m, "size_only", "sizeOnly"); ok {
		p.SizeOnly = v
	}
	if v, ok := asBool(m, "update_mode", "updateMode"); ok {
		p.UpdateMode = v
	}
	if v, ok := asBool(m, "ignore_existing", "ignoreExisting"); ok {
		p.IgnoreExisting = v
	}

	if v, ok := asString(m, "delete_timing", "deleteTiming"); ok {
		p.DeleteTiming = v
	}

	if v, ok := asString(m, "conflict_resolution", "conflictResolution"); ok {
		p.ConflictResolution = v
	}
	if v, ok := asBool(m, "resilient"); ok {
		p.Resilient = v
	}
	if v, ok := asString(m, "max_lock", "maxLock"); ok {
		p.MaxLock = v
	}
	if v, ok := asBool(m, "check_access", "checkAccess"); ok {
		p.CheckAccess = v
	}
	if v, ok := asString(m, "conflict_loser", "conflictLoser"); ok {
		p.ConflictLoser = v
	}
	if v, ok := asString(m, "conflict_suffix", "conflictSuffix"); ok {
		p.ConflictSuffix = v
	}

	return p
}

func intPtr(v int) *int          { return &v }
func floatPtr(v float64) *float64 { return &v }

func asBool(m map[string]any, keys ...string) (bool, bool) {
	for _, k := range keys {
		if v, ok := m[k]; ok {
			switch t := v.(type) {
			case bool:
				return t, true
			}
		}
	}
	return false, false
}

func asInt(m map[string]any, keys ...string) (int, bool) {
	for _, k := range keys {
		if v, ok := m[k]; ok {
			switch t := v.(type) {
			case float64:
				return int(t), true
			case int:
				return t, true
			case string:
				if n, err := strconv.Atoi(strings.TrimSpace(t)); err == nil {
					return n, true
				}
			}
		}
	}
	return 0, false
}

func asFloat(m map[string]any, keys ...string) (float64, bool) {
	for _, k := range keys {
		if v, ok := m[k]; ok {
			switch t := v.(type) {
			case float64:
				return t, true
			case int:
				return float64(t), true
			case string:
				if n, err := strconv.ParseFloat(strings.TrimSpace(t), 64); err == nil {
					return n, true
				}
			}
		}
	}
	return 0, false
}

func asString(m map[string]any, keys ...string) (string, bool) {
	for _, k := range keys {
		if v, ok := m[k]; ok {
			if s, ok := v.(string); ok && strings.TrimSpace(s) != "" {
				return strings.TrimSpace(s), true
			}
		}
	}
	return "", false
}

func asStringSlice(m map[string]any, keys ...string) ([]string, bool) {
	for _, k := range keys {
		if v, ok := m[k]; ok {
			switch t := v.(type) {
			case []any:
				out := make([]string, 0, len(t))
				for _, x := range t {
					if s, ok := x.(string); ok && strings.TrimSpace(s) != "" {
						out = append(out, strings.TrimSpace(s))
					}
				}
				return out, true
			case []string:
				return append([]string(nil), t...), true
			}
		}
	}
	return nil, false
}
