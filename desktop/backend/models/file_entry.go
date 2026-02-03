package models

// FileEntry represents a file or directory in a remote listing
type FileEntry struct {
	Path     string `json:"path"`
	Name     string `json:"name"`
	Size     int64  `json:"size"`
	ModTime  string `json:"mod_time"`
	IsDir    bool   `json:"is_dir"`
	MimeType string `json:"mime_type,omitempty"`
}

// QuotaInfo contains storage quota information for a remote
type QuotaInfo struct {
	Total   int64 `json:"total"`
	Used    int64 `json:"used"`
	Free    int64 `json:"free"`
	Trashed int64 `json:"trashed,omitempty"`
}

// ListOptions contains options for listing files on a remote
type ListOptions struct {
	Recursive bool   `json:"recursive"`
	MaxDepth  int    `json:"max_depth"`
	SortBy    string `json:"sort_by"` // "name", "size", "mod_time"
}
