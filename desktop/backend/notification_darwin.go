//go:build darwin

package backend

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
					NSLog(@"NG Drive: Notification auth error: %@", error);
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
