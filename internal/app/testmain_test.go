package app

import (
	"os"
	"path/filepath"
	"testing"
)

// TestMain puts a harmless fake rclone ahead of PATH so no test in this
// package can reach a real rclone binary (which could start an OAuth sign-in).
// The fake only answers the calls New() makes (`version`, `listremotes`) and
// otherwise exits 0; it never talks to a provider.
func TestMain(m *testing.M) {
	dir, err := os.MkdirTemp("", "gn-drive-app-fake-rclone-")
	if err != nil {
		panic(err)
	}
	script := "#!/bin/sh\nexit 0\n"
	if err := os.WriteFile(filepath.Join(dir, "rclone"), []byte(script), 0o755); err != nil {
		panic(err)
	}
	if err := os.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH")); err != nil {
		panic(err)
	}
	code := m.Run()
	_ = os.RemoveAll(dir)
	os.Exit(code)
}
