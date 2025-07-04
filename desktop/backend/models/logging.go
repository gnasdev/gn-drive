package models

import (
	"encoding/json"
	"time"
)

// LogLevel represents the severity level of a log entry
type LogLevel string

const (
	DEBUG    LogLevel = "DEBUG"
	INFO     LogLevel = "INFO"
	WARN     LogLevel = "WARN"
	ERROR    LogLevel = "ERROR"
	CRITICAL LogLevel = "CRITICAL"
)

// String returns the string representation of LogLevel
func (l LogLevel) String() string {
	return string(l)
}

// FrontendLogEntry represents a log entry from the frontend
type FrontendLogEntry struct {
	Level       LogLevel  `json:"level"`
	Message     string    `json:"message"`
	Context     string    `json:"context,omitempty"`
	Details     string    `json:"details,omitempty"`
	Timestamp   time.Time `json:"timestamp"`
	BrowserInfo string    `json:"browser_info,omitempty"`
	UserAgent   string    `json:"user_agent,omitempty"`
	StackTrace  string    `json:"stack_trace,omitempty"`
	URL         string    `json:"url,omitempty"`
	Component   string    `json:"component,omitempty"`
	TraceID     string    `json:"trace_id,omitempty"`
}

// ToJSON converts the log entry to JSON bytes
func (f *FrontendLogEntry) ToJSON() ([]byte, error) {
	return json.Marshal(f)
}

// FromJSON creates a FrontendLogEntry from JSON bytes
func (f *FrontendLogEntry) FromJSON(data []byte) error {
	return json.Unmarshal(data, f)
}

// IsValid checks if the log entry has required fields
func (f *FrontendLogEntry) IsValid() bool {
	return f.Level != "" && f.Message != "" && !f.Timestamp.IsZero()
}

// GetSeverityLevel returns a numeric severity level for sorting/filtering
func (f *FrontendLogEntry) GetSeverityLevel() int {
	switch f.Level {
	case DEBUG:
		return 1
	case INFO:
		return 2
	case WARN:
		return 3
	case ERROR:
		return 4
	case CRITICAL:
		return 5
	default:
		return 0
	}
}
