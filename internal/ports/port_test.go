package ports

import (
	"net"
	"testing"
)

func TestDefaultPort_InDynamicRange(t *testing.T) {
	// IANA dynamic/private ports: 49152–65535 — low conflict with system services.
	if DefaultPort < 49152 || DefaultPort > 65535 {
		t.Errorf("DefaultPort = %d, want in 49152–65535", DefaultPort)
	}
	if DefaultPort != 53241 {
		t.Errorf("DefaultPort = %d, want 53241 (keep docs/dev tooling in sync)", DefaultPort)
	}
}

func TestAllocate_DefaultPort(t *testing.T) {
	l, port, err := Allocate()
	if err != nil {
		t.Skipf("default port in use: %v", err)
	}
	defer l.Close()
	if port != DefaultPort {
		t.Errorf("port = %d, want DefaultPort %d", port, DefaultPort)
	}
	if l.Port != port {
		t.Errorf("listener.Port = %d, want %d", l.Port, port)
	}
	if !startsWith(l.Addr().String(), "127.0.0.1:") {
		t.Errorf("addr = %q, want 127.0.0.1:...", l.Addr().String())
	}
	c, err := net.Dial("tcp", l.Addr().String())
	if err != nil {
		t.Fatalf("dial: %v", err)
	}
	_ = c.Close()
}

func TestAllocatePort_SpecificPort(t *testing.T) {
	// Bind an ephemeral OS port first to learn a free number, then re-bind it
	// via AllocatePort (simulates --port override).
	tmp, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	port := tmp.Addr().(*net.TCPAddr).Port
	_ = tmp.Close()

	l, got, err := AllocatePort(port)
	if err != nil {
		t.Skipf("could not reclaim port %d: %v", port, err)
	}
	defer l.Close()
	if got != port {
		t.Errorf("got port = %d, want %d", got, port)
	}
}

func TestAllocatePort_ZeroMapsToDefault(t *testing.T) {
	l, port, err := AllocatePort(0)
	if err != nil {
		t.Skipf("default port in use: %v", err)
	}
	defer l.Close()
	if port != DefaultPort {
		t.Errorf("port = %d, want DefaultPort %d", port, DefaultPort)
	}
}

func TestAllocatePort_InUseFails(t *testing.T) {
	// Use a free ephemeral port so we don't fight DefaultPort if something else holds it.
	tmp, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	port := tmp.Addr().(*net.TCPAddr).Port
	// Keep tmp open so the second bind fails.
	defer tmp.Close()

	_, _, err = AllocatePort(port)
	if err == nil {
		t.Fatal("expected error when binding to in-use port")
	}
}

func startsWith(s, prefix string) bool {
	return len(s) >= len(prefix) && s[:len(prefix)] == prefix
}
