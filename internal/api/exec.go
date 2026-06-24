// Package api provides the HTTP API server for the web UI.
package api

import "os"

// ownExe returns the absolute path to the running binary. It is a variable
// so tests can override the actual lookup.
var ownExe = func() (string, error) {
	return os.Executable()
}
