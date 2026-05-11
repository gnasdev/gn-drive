//go:build darwin

package backend

// GN Drive note: Supports the Go backend for notification darwin.

/*
#cgo CFLAGS: -x objective-c
#cgo LDFLAGS: -framework Cocoa -framework UserNotifications

#include <stdlib.h>
#import <Cocoa/Cocoa.h>
#import <UserNotifications/UserNotifications.h>

static int hasBundleId() {
	NSString *bid = [[NSBundle mainBundle] bundleIdentifier];
	return (bid != nil && [bid length] > 0) ? 1 : 0;
}

static void requestNotifAuth() {
	dispatch_async(dispatch_get_main_queue(), ^{
		[[UNUserNotificationCenter currentNotificationCenter]
			requestAuthorizationWithOptions:(UNAuthorizationOptionAlert | UNAuthorizationOptionSound)
			completionHandler:^(BOOL granted, NSError *error) {
				if (error) {
					NSLog(@"GN Drive: Notification auth error: %@", error);
				}
			}];
	});
}

static void sendNotif(const char *title, const char *body) {
	NSString *nsTitle = [[NSString alloc] initWithUTF8String:title];
	NSString *nsBody = [[NSString alloc] initWithUTF8String:body];

	dispatch_async(dispatch_get_main_queue(), ^{
		UNMutableNotificationContent *content = [[UNMutableNotificationContent alloc] init];
		content.title = nsTitle;
		content.body = nsBody;
		content.sound = [UNNotificationSound defaultSound];

		UNNotificationRequest *request = [UNNotificationRequest
			requestWithIdentifier:[[NSUUID UUID] UUIDString]
			content:content
			trigger:nil];

		[[UNUserNotificationCenter currentNotificationCenter]
			addNotificationRequest:request
			withCompletionHandler:nil];

		[nsTitle release];
		[nsBody release];
	});
}

static int getNotifAuthStatus() {
	__block NSInteger status = -1;
	dispatch_semaphore_t sem = dispatch_semaphore_create(0);

	[[UNUserNotificationCenter currentNotificationCenter]
		getNotificationSettingsWithCompletionHandler:^(UNNotificationSettings *settings) {
			status = settings.authorizationStatus;
			dispatch_semaphore_signal(sem);
		}];

	dispatch_time_t timeout = dispatch_time(DISPATCH_TIME_NOW, 2 * NSEC_PER_SEC);
	if (dispatch_semaphore_wait(sem, timeout) != 0) {
		return -2;
	}

	return (int)status;
}
*/
import "C"

import (
	"sync"
	"unsafe"
)

var notifInitOnce sync.Once

// NativeNotificationAvailable returns true if native macOS notifications can be used
// (requires app bundle with CFBundleIdentifier).
func NativeNotificationAvailable() bool {
	return C.hasBundleId() == 1
}

// InitNativeNotifications requests macOS notification authorization.
func InitNativeNotifications() {
	notifInitOnce.Do(func() {
		C.requestNotifAuth()
	})
}

// SendNativeNotification sends a macOS notification via UNUserNotificationCenter.
func SendNativeNotification(title, body string) {
	cTitle := C.CString(title)
	cBody := C.CString(body)
	defer C.free(unsafe.Pointer(cTitle))
	defer C.free(unsafe.Pointer(cBody))

	C.sendNotif(cTitle, cBody)
}

// GetNativeNotificationAuthorizationStatus returns the macOS notification permission state.
func GetNativeNotificationAuthorizationStatus() string {
	switch int(C.getNotifAuthStatus()) {
	case 0:
		return "not_determined"
	case 1:
		return "denied"
	case 2:
		return "authorized"
	case 3:
		return "provisional"
	case 4:
		return "ephemeral"
	case -2:
		return "timeout"
	default:
		return "unknown"
	}
}
