package models

type Remote struct {
	Name  string         `json:"name"`
	Type  string         `json:"type"`
	Token map[string]any `json:"token"`
}

type Remotes []Remote
