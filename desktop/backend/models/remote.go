package models

// GN Drive note: Defines remote data structures shared by backend services and Wails bindings.

type Remote struct {
	Name  string         `json:"name"`
	Type  string         `json:"type"`
	Token map[string]any `json:"token"`
}

type Remotes []Remote
