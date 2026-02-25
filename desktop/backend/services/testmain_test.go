package services

import (
	"os"
	"testing"
)

func TestMain(m *testing.M) {
	// Set up test database in a temp directory
	tmpDir, err := os.MkdirTemp("", "ng-drive-test-*")
	if err != nil {
		panic("failed to create temp dir: " + err.Error())
	}
	defer os.RemoveAll(tmpDir)

	SetSharedConfig(&SharedConfig{
		ConfigDir:  tmpDir,
		WorkingDir: tmpDir,
	})

	if err := InitDatabase(); err != nil {
		panic("failed to init database: " + err.Error())
	}
	defer CloseDatabase()

	os.Exit(m.Run())
}
