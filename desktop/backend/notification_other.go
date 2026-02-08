//go:build !darwin

package backend

// InitNativeNotifications is a no-op on non-macOS platforms.
func InitNativeNotifications() {}

// SendNativeNotification is a no-op on non-macOS platforms.
func SendNativeNotification(title, body string) {}
