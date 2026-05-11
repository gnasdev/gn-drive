//go:build darwin

package services

// GN Drive note: Coordinates the notification darwin service behavior exposed to the desktop application.

import (
	be "desktop/backend"
	"log"
	"sync"

	"github.com/gen2brain/beeep"
)

var (
	useNative     bool
	notifInitOnce sync.Once
)

func sendPlatformNotification(title, body string) error {
	notifInitOnce.Do(func() {
		useNative = be.NativeNotificationAvailable()
		if useNative {
			be.InitNativeNotifications()
			log.Println("GN Drive: Using native macOS notifications")
		} else {
			log.Println("GN Drive: No bundle ID (dev mode), using beeep notifications")
		}
	})

	if useNative {
		be.SendNativeNotification(title, body)
		return nil
	}

	return beeep.Notify(title, body, "")
}

func getPlatformNotificationStatus() NotificationStatus {
	if !be.NativeNotificationAvailable() {
		return NotificationStatus{
			NativeProvider:  "beeep",
			NativeAvailable: false,
			Permission:      "unmanaged",
			Detail:          "macOS bundle identifier is missing; development mode uses beeep fallback notifications",
		}
	}

	return NotificationStatus{
		NativeProvider:  "usernotifications",
		NativeAvailable: true,
		Permission:      be.GetNativeNotificationAuthorizationStatus(),
	}
}
