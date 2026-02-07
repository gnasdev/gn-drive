package models

// Flow represents a sync workflow containing sequential operations
type Flow struct {
	Id              string      `json:"id"`
	Name            string      `json:"name"`
	IsCollapsed     bool        `json:"is_collapsed"`
	ScheduleEnabled bool        `json:"schedule_enabled"`
	CronExpr        string      `json:"cron_expr,omitempty"`
	SortOrder       int         `json:"sort_order"`
	Operations      []Operation `json:"operations"`
	CreatedAt       string      `json:"created_at,omitempty"`
	UpdatedAt       string      `json:"updated_at,omitempty"`
}

// Operation represents a single sync operation between two remotes
type Operation struct {
	Id                 string   `json:"id"`
	FlowId             string   `json:"flow_id"`
	SourceRemote       string   `json:"source_remote"`
	SourcePath         string   `json:"source_path"`
	TargetRemote       string   `json:"target_remote"`
	TargetPath         string   `json:"target_path"`
	Action             string   `json:"action"`
	Parallel           int      `json:"parallel,omitempty"`
	Bandwidth          string   `json:"bandwidth,omitempty"`
	IncludedPaths      []string `json:"included_paths,omitempty"`
	ExcludedPaths      []string `json:"excluded_paths,omitempty"`
	ConflictResolution string   `json:"conflict_resolution,omitempty"`
	DryRun             bool     `json:"dry_run,omitempty"`
	IsExpanded         bool     `json:"is_expanded"`
	SortOrder          int      `json:"sort_order"`
}
