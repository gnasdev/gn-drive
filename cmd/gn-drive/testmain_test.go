package main

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

// TestMain installs a fake rclone ahead of any real binary on PATH so no test
// can shell out to the user's rclone or start an OAuth sign-in. The fake
// blocks `authorize` and `config create` for token-based backends and records
// the attempt; any such attempt fails the whole test binary.
func TestMain(m *testing.M) {
	dir, err := os.MkdirTemp("", "gn-drive-fake-rclone-")
	if err != nil {
		panic(err)
	}
	defer os.RemoveAll(dir)

	fake := filepath.Join(dir, "rclone")
	if err := os.WriteFile(fake, []byte(fakeRcloneScript), 0o755); err != nil {
		panic(err)
	}
	violation := filepath.Join(dir, "oauth-violation")
	if err := os.Setenv("GN_DRIVE_TEST_FAKE_RCLONE", fake); err != nil {
		panic(err)
	}
	if err := os.Setenv("GN_DRIVE_TEST_OAUTH_VIOLATION", violation); err != nil {
		panic(err)
	}
	if err := os.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH")); err != nil {
		panic(err)
	}

	code := m.Run()

	if data, err := os.ReadFile(violation); err == nil {
		fmt.Fprintf(os.Stderr, "FATAL: a test attempted a real OAuth rclone invocation:\n%s\n", data)
		code = 1
	}
	os.Exit(code)
}

// fakeRclone returns the guarded fake binary from TestMain and gives the test
// its own state file so `config create`/`listremotes` round-trip.
func fakeRclone(t *testing.T) string {
	t.Helper()
	bin := os.Getenv("GN_DRIVE_TEST_FAKE_RCLONE")
	if bin == "" {
		t.Fatal("fake rclone not installed by TestMain")
	}
	t.Setenv("GN_DRIVE_TEST_RCLONE_STATE", filepath.Join(t.TempDir(), "remotes"))
	return bin
}

const fakeRcloneScript = `#!/bin/sh
state="${GN_DRIVE_TEST_RCLONE_STATE:-}"
violation="${GN_DRIVE_TEST_OAUTH_VIOLATION:-}"
args=" $* "

case "$args" in
  *" authorize "*)
    echo "GUARD: rclone authorize blocked in tests" >&2
    [ -n "$violation" ] && echo "authorize $*" >> "$violation"
    exit 99 ;;
esac

if [ "$1" = "config" ] && [ "$2" = "create" ]; then
  type="$4"
  case "$type" in
    drive|dropbox|onedrive|box|pcloud|yandex|googlephotos|iclouddrive|mega|mailru|sharepoint|jottacloud|koofr|opendrive|pikpak|sugarsync|uptobox|zoho|premiumizeme|seafile|sia|qingstor|putio)
      echo "GUARD: OAuth remote type '$type' blocked in tests" >&2
      [ -n "$violation" ] && echo "config create $*" >> "$violation"
      exit 98 ;;
  esac
  [ -n "$state" ] && printf '%s:\n' "$3" >> "$state"
  exit 0
fi

case "$args" in
  *" listremotes "*) [ -n "$state" ] && cat "$state" 2>/dev/null; exit 0 ;;
  *" config dump "*) echo '{}'; exit 0 ;;
  *" config delete "*) exit 0 ;;
  *" lsd "*) exit 0 ;;
  *" version "*) echo "rclone v1.99.0-test"; exit 0 ;;
esac

exit 0
`
