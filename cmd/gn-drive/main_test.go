package main

import (
	"bytes"
	"io"
	"os"
	"strings"
	"testing"
)

// withExit replaces os.Exit for the duration of fn and returns the captured
// exit code. It also captures stderr to a buffer.
func withExit(t *testing.T, fn func()) (int, *bytes.Buffer) {
	t.Helper()
	origExit := osExit
	origStderr := os.Stderr
	r, w, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	os.Stderr = w
	stderrBuf := &bytes.Buffer{}
	done := make(chan struct{})
	go func() {
		_, _ = io.Copy(stderrBuf, r)
		close(done)
	}()
	var code int
	osExit = func(c int) { code = c }
	defer func() {
		osExit = origExit
		os.Stderr = origStderr
		w.Close()
		<-done
	}()
	fn()
	return code, stderrBuf
}

func TestFatal_OutputsAndExits(t *testing.T) {
	code, buf := withExit(t, func() {
		fatal("oops: %s", "boom")
	})
	if code != 1 {
		t.Errorf("exit code = %d, want 1", code)
	}
	if !strings.Contains(buf.String(), "fatal: oops: boom") {
		t.Errorf("stderr = %q, want 'fatal: oops: boom'", buf.String())
	}
}

func TestFatal_NoArgs(t *testing.T) {
	code, buf := withExit(t, func() {
		fatal("simple message")
	})
	if code != 1 {
		t.Errorf("exit code = %d, want 1", code)
	}
	if !strings.Contains(buf.String(), "fatal: simple message") {
		t.Errorf("stderr = %q", buf.String())
	}
}

// TestMain_Version exercises the main() entry point with `version` arg.
func TestMain_Version(t *testing.T) {
	origArgs := os.Args
	defer func() { os.Args = origArgs }()
	os.Args = []string{"gn-drive", "version"}

	// Capture stdout to verify version output.
	origStdout := os.Stdout
	r, w, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	os.Stdout = w
	stdoutBuf := &bytes.Buffer{}
	done := make(chan struct{})
	go func() {
		_, _ = io.Copy(stdoutBuf, r)
		close(done)
	}()

	main()
	// Restore stdout and close pipe so the reader can finish before we read.
	os.Stdout = origStdout
	w.Close()
	<-done

	if !strings.Contains(stdoutBuf.String(), "gn-drive") {
		t.Errorf("expected version output, got: %q", stdoutBuf.String())
	}
}

// TestMain_HelpFlag tests the --help flag (which uses stdout) — main() also
// covers this code path since it uses the same root command.
func TestMain_HelpFlag(t *testing.T) {
	origArgs := os.Args
	defer func() { os.Args = origArgs }()
	os.Args = []string{"gn-drive", "--help"}

	// Capture stdout (cobra writes help to stdout when --help is passed).
	r, w, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	origStdout := os.Stdout
	os.Stdout = w
	stdoutBuf := &bytes.Buffer{}
	done := make(chan struct{})
	go func() {
		_, _ = io.Copy(stdoutBuf, r)
		close(done)
	}()

	main()
	os.Stdout = origStdout
	w.Close()
	<-done

	out := stdoutBuf.String()
	if !strings.Contains(out, "Subcommands:") && !strings.Contains(out, "Available Commands") && !strings.Contains(out, "Flags:") {
		t.Errorf("expected help output, got: %q", out)
	}
}

// TestMain_UnknownSubcommand ensures main() doesn't panic on unknown sub.
func TestMain_UnknownSubcommand(t *testing.T) {
	origArgs := os.Args
	defer func() { os.Args = origArgs }()
	os.Args = []string{"gn-drive", "no-such-sub"}

	// We need to catch the os.Exit(1) call.
	code, _ := withExit(t, func() {
		// Also silence stdout.
		origStdout := os.Stdout
		os.Stdout, _ = os.Open(os.DevNull)
		defer func() { os.Stdout = origStdout }()
		main()
	})
	// Unknown subcommand → cobra returns error → main exits with 1.
	_ = code
}
