//go:build !darwin

package services

import "github.com/gen2brain/beeep"

func sendPlatformNotification(title, body string) error {
	return beeep.Notify(title, body, "")
}
