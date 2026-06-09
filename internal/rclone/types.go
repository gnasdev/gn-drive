// Shared types for the rclone package.
package rclone

import "time"

// FileEntry mirrors rclone's lsjson output.
type FileEntry struct {
	Name     string `json:"Name"`
	Size     int64  `json:"Size"`
	MimeType string `json:"MimeType"`
	IsDir    bool   `json:"IsDir"`
	ModTime  string `json:"ModTime"`
	Path     string `json:"Path"`
	ID       string `json:"ID"`
}

// QuotaInfo describes a remote's storage quota.
type QuotaInfo struct {
	Used  int64 `json:"used"`
	Total int64 `json:"total"`
	Free  int64 `json:"free"`
}

// ResolverPolicy maps profile + remote types to per-provider rate limits.
// Mirrors desktop/backend/rclone/resolver_policy.go semantics but simplified
// for the shell-out wrapper.
type ResolverPolicy struct {
	Transfers int
	Checkers  int
	TPSLimit  float64
}

// ApplyResolverPolicy returns a policy based on the providers involved.
// Cloud-to-cloud gets low parallelism + TPS limit; local-to-cloud is permissive.
func ApplyResolverPolicy(sourceType, destType string) ResolverPolicy {
	rateLimited := isRateLimited(sourceType) || isRateLimited(destType)
	if rateLimited {
		return ResolverPolicy{Transfers: 4, Checkers: 4, TPSLimit: 4}
	}
	return ResolverPolicy{Transfers: 8, Checkers: 8, TPSLimit: 0}
}

func isRateLimited(t string) bool {
	switch t {
	case "drive", "onedrive", "dropbox", "box", "icloud", "iclouddrive",
		"googlephotos", "mega", "pcloud", "yandex", "mailru", "sharepoint":
		return true
	}
	return false
}

// nowUnix is overridable in tests.
var timeNowFunc = func() int64 { return time.Now().Unix() }
