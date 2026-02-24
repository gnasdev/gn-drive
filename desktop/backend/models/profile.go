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
	MaxAge           string `json:"max_age,omitempty"`             // --max-age e.g. "24h","7d"
	MinAge           string `json:"min_age,omitempty"`             // --min-age e.g. "24h","7d"
	MaxDepth         *int   `json:"max_depth,omitempty"`           // --max-depth (nil=unlimited)
	DeleteExcluded   bool   `json:"delete_excluded,omitempty"`     // --delete-excluded

	// Safety
	MaxDelete           *int   `json:"max_delete,omitempty"`            // max files to delete per sync (nil=unlimited)
	Immutable           bool   `json:"immutable,omitempty"`             // prevent overwriting existing files
	ConflictResolution  string `json:"conflict_resolution,omitempty"`   // bisync conflict strategy: "newer","older","larger","smaller","path1","path2"
	DryRun              bool   `json:"dry_run,omitempty"`               // --dry-run (preview only)
	MaxTransfer         string `json:"max_transfer,omitempty"`          // --max-transfer e.g. "10G"
	MaxDeleteSize       string `json:"max_delete_size,omitempty"`       // --max-delete-size e.g. "1G"
	Suffix              string `json:"suffix,omitempty"`                // --suffix for changed files
	SuffixKeepExtension bool   `json:"suffix_keep_extension,omitempty"` // --suffix-keep-extension

	// Performance
	MultiThreadStreams *int     `json:"multi_thread_streams,omitempty"` // concurrent streams per file transfer
	BufferSize         string   `json:"buffer_size,omitempty"`          // in-memory buffer per transfer e.g. "16M","64M"
	Retries            *int     `json:"retries,omitempty"`              // number of retries on failure
	LowLevelRetries    *int     `json:"low_level_retries,omitempty"`    // protocol-level retries
	MaxDuration        string   `json:"max_duration,omitempty"`         // max operation time e.g. "1h30m"
	CheckFirst         bool     `json:"check_first,omitempty"`          // --check-first
	OrderBy            string   `json:"order_by,omitempty"`             // --order-by e.g. "size,desc"
	RetriesSleep       string   `json:"retries_sleep,omitempty"`        // --retries-sleep e.g. "10s"
	TpsLimit           *float64 `json:"tps_limit,omitempty"`            // --tpslimit
	ConnTimeout        string   `json:"conn_timeout,omitempty"`         // --contimeout e.g. "30s"
	IoTimeout          string   `json:"io_timeout,omitempty"`           // --timeout e.g. "5m"

	// Comparison
	SizeOnly       bool `json:"size_only,omitempty"`       // --size-only
	UpdateMode     bool `json:"update_mode,omitempty"`     // --update (skip newer destination files)
	IgnoreExisting bool `json:"ignore_existing,omitempty"` // --ignore-existing

	// Sync-specific
	DeleteTiming string `json:"delete_timing,omitempty"` // "before","during","after" (--delete-before/during/after)

	// Bisync-specific
	Resilient      bool   `json:"resilient,omitempty"`       // --resilient
	MaxLock        string `json:"max_lock,omitempty"`        // --max-lock e.g. "15m"
	CheckAccess    bool   `json:"check_access,omitempty"`    // --check-access
	ConflictLoser  string `json:"conflict_loser,omitempty"`  // --conflict-loser: "num","pathname","delete"
	ConflictSuffix string `json:"conflict_suffix,omitempty"` // --conflict-suffix

	// Encryption (on-the-fly crypt wrapping, runtime only - not persisted to DB)
	EncryptSource    bool   `json:"encrypt_source,omitempty"`    // Wrap source with crypt remote
	EncryptDest      bool   `json:"encrypt_dest,omitempty"`      // Wrap destination with crypt remote
	EncryptPassword  string `json:"encrypt_password,omitempty"`  // Crypt password
	EncryptPassword2 string `json:"encrypt_password2,omitempty"` // Salt password (optional)
	EncryptFilename  string `json:"encrypt_filename,omitempty"`  // "standard", "obfuscate", "off"
	EncryptDirectory bool   `json:"encrypt_directory,omitempty"` // Encrypt directory names
}

// StripEncryptPasswords clears encryption passwords so they are not persisted to DB.
func (p *Profile) StripEncryptPasswords() {
	p.EncryptPassword = ""
	p.EncryptPassword2 = ""
}

type Profiles []Profile
