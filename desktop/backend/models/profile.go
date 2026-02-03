package models

type Profile struct {
	Name          string   `json:"name"`
	From          string   `json:"from"`
	To            string   `json:"to"`
	IncludedPaths []string `json:"included_paths"`
	ExcludedPaths []string `json:"excluded_paths"`
	Bandwidth     int      `json:"bandwidth" default:"5M"`
	Parallel      int      `json:"parallel" default:"16"`
	BackupPath    string   `json:"backup_path"`
	CachePath     string   `json:"cache_path"`

	// Filtering
	MinSize          string `json:"min_size,omitempty"`            // rclone size suffix e.g. "100k", "10M", "1G"
	MaxSize          string `json:"max_size,omitempty"`            // rclone size suffix
	FilterFromFile   string `json:"filter_from_file,omitempty"`    // path to filter rules file (--filter-from)
	ExcludeIfPresent string `json:"exclude_if_present,omitempty"`  // marker filename e.g. ".nosync"
	UseRegex         bool   `json:"use_regex,omitempty"`           // use regex for include/exclude patterns

	// Safety
	MaxDelete          *int   `json:"max_delete,omitempty"`           // max files to delete per sync (nil=unlimited)
	Immutable          bool   `json:"immutable,omitempty"`            // prevent overwriting existing files
	ConflictResolution string `json:"conflict_resolution,omitempty"`  // bisync conflict strategy: "newer","older","larger","smaller","path1","path2"

	// Performance
	MultiThreadStreams *int   `json:"multi_thread_streams,omitempty"` // concurrent streams per file transfer
	BufferSize         string `json:"buffer_size,omitempty"`          // in-memory buffer per transfer e.g. "16M","64M"
	FastList           bool   `json:"fast_list,omitempty"`            // use recursive list (fewer API calls, more memory)
	Retries            *int   `json:"retries,omitempty"`              // number of retries on failure
	LowLevelRetries    *int   `json:"low_level_retries,omitempty"`    // protocol-level retries
	MaxDuration        string `json:"max_duration,omitempty"`         // max operation time e.g. "1h30m"
}

type Profiles []Profile
