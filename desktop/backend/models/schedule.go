package models

import "time"

// ScheduleEntry represents a scheduled sync operation
type ScheduleEntry struct {
	Id          string     `json:"id"`
	ProfileName string     `json:"profile_name"`
	Action      string     `json:"action"`      // "pull", "push", "bi", "bi-resync", "copy", "move"
	CronExpr    string     `json:"cron_expr"`   // cron expression e.g. "0 */6 * * *"
	Enabled     bool       `json:"enabled"`
	LastRun     *time.Time `json:"last_run,omitempty"`
	NextRun     *time.Time `json:"next_run,omitempty"`
	LastResult  string     `json:"last_result,omitempty"` // "success", "failed", "cancelled"
	CreatedAt   time.Time  `json:"created_at"`
}
