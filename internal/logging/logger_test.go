package logging

import (
	"bytes"
	"context"
	"encoding/json"
	"log/slog"
	"os"
	"strings"
	"sync"
	"testing"
)

func TestNew_ForegroundTextHandler(t *testing.T) {
	l := New(ModeForeground)
	if l == nil {
		t.Fatal("New returned nil")
	}
	defer l.Close()
	if l.Mode() != ModeForeground {
		t.Errorf("Mode = %q, want ModeForeground", l.Mode())
	}
	if l.Logger == nil {
		t.Error("Logger is nil")
	}
}

func TestNew_ServiceJSONHandler(t *testing.T) {
	l := New(ModeService)
	if l == nil {
		t.Fatal("New returned nil")
	}
	defer l.Close()
	if l.Mode() != ModeService {
		t.Errorf("Mode = %q, want ModeService", l.Mode())
	}
}

func TestNew_DefaultsToForegroundForUnknown(t *testing.T) {
	l := New(Mode("mystery"))
	defer l.Close()
	if l.Mode() != Mode("mystery") {
		t.Errorf("Mode should preserve input; got %q", l.Mode())
	}
}

func TestLogger_Write_ProducesOutput(t *testing.T) {
	// Replace stderr to capture.
	r, w, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	origStderr := os.Stderr
	os.Stderr = w
	defer func() { os.Stderr = origStderr }()

	var buf bytes.Buffer
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		_, _ = buf.ReadFrom(r)
	}()

	l := New(ModeForeground)
	l.Info("hello world", "k", "v")
	l.Close()
	w.Close()
	wg.Wait()

	if !strings.Contains(buf.String(), "hello world") {
		t.Errorf("log output %q should contain message", buf.String())
	}
}

func TestLogger_ServiceJSON(t *testing.T) {
	r, w, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	origStderr := os.Stderr
	os.Stderr = w
	defer func() { os.Stderr = origStderr }()

	var buf bytes.Buffer
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		_, _ = buf.ReadFrom(r)
	}()

	l := New(ModeService)
	l.Info("payload", "count", 42)
	l.Close()
	w.Close()
	wg.Wait()

	out := strings.TrimSpace(buf.String())
	if out == "" {
		t.Fatal("expected non-empty log line")
	}
	// Must be valid JSON.
	var parsed map[string]any
	if err := json.Unmarshal([]byte(out), &parsed); err != nil {
		t.Fatalf("log line is not valid JSON: %v\n%s", err, out)
	}
	if parsed["msg"] != "payload" {
		t.Errorf("msg = %v, want 'payload'", parsed["msg"])
	}
	if parsed["count"] != float64(42) {
		t.Errorf("count = %v, want 42", parsed["count"])
	}
}

func TestWithContext_ReturnsLogger(t *testing.T) {
	l := New(ModeForeground)
	defer l.Close()
	got := l.WithContext(context.Background())
	if got == nil {
		t.Error("WithContext returned nil")
	}
	if got != l {
		t.Error("WithContext should be a no-op for now; got different instance")
	}
}

func TestClose_Idempotent(t *testing.T) {
	l := New(ModeForeground)
	if err := l.Close(); err != nil {
		t.Errorf("first Close: %v", err)
	}
	if err := l.Close(); err != nil {
		t.Errorf("second Close: %v", err)
	}
}

// TestClose_CallsCancel covers the l.cancel != nil branch in Close() by
// setting the unexported cancel field to a no-op function.
func TestClose_CallsCancel(t *testing.T) {
	l := New(ModeForeground)
	called := false
	l.cancel = func() { called = true }
	if err := l.Close(); err != nil {
		t.Errorf("Close: %v", err)
	}
	if !called {
		t.Error("cancel was not invoked when set")
	}
}

func TestModeConstants(t *testing.T) {
	if ModeForeground == "" || ModeService == "" {
		t.Error("mode constants must be non-empty")
	}
	if ModeForeground == ModeService {
		t.Error("mode constants must differ")
	}
}

// Compile-time check.
var _ *slog.Logger
