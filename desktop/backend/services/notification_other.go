//go:build !darwin

package services

// GN Drive note: Coordinates the notification other service behavior exposed to the desktop application.

import "github.com/gen2brain/beeep"

func sendPlatformNotification(title, body string) error {
	return beeep.Notify(title, body, "")
}

func getPlatformNotificationStatus() NotificationStatus {
	return NotificationStatus{
		NativeProvider:  "beeep",
		NativeAvailable: true,
		Permission:      "unmanaged",
	}
}
