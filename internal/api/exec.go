// Package api provides the HTTP API server for the web UI.
package api

import "os"

// ownExe returns the absolute path to the running binary.
func ownExe() (string, error) {
	return os.Executable()
}
