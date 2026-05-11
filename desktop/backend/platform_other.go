//go:build !darwin

package backend

// GN Drive note: Supports the Go backend for platform other.

// HideFromDock is a no-op on non-macOS platforms.
func HideFromDock() {}

// ShowInDock is a no-op on non-macOS platforms.
func ShowInDock() {}
