package backend

/*
#cgo CFLAGS: -x objective-c
#cgo LDFLAGS: -framework Cocoa
#import <Cocoa/Cocoa.h>

// setDockVisible changes the macOS activation policy to show/hide the Dock icon.
// policy 0 = Regular (show in Dock), 1 = Accessory (hide from Dock)
static void setDockVisible(int visible) {
	dispatch_async(dispatch_get_main_queue(), ^{
		if (visible) {
			[NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
			[NSApp activateIgnoringOtherApps:YES];
		} else {
			[NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];
		}
	});
}
*/
import "C"

// HideFromDock hides the app from the macOS Dock.
func HideFromDock() {
	C.setDockVisible(0)
}

// ShowInDock shows the app in the macOS Dock.
func ShowInDock() {
	C.setDockVisible(1)
}
