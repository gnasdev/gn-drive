//go:build !darwin

package backend

// GN Drive note: Supports the Go backend for notification other.

// InitNativeNotifications is a no-op on non-macOS platforms.
func InitNativeNotifications() {}

// SendNativeNotification is a no-op on non-macOS platforms.
func SendNativeNotification(title, body string) {}
