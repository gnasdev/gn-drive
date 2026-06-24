package ports

import (
	"net"
	"testing"
)

func TestAllocate_AutoPort(t *testing.T) {
	l, port, err := Allocate()
	if err != nil {
		t.Fatalf("Allocate: %v", err)
	}
	defer l.Close()
	if port == 0 {
		t.Error("port = 0, want kernel-assigned non-zero port")
	}
	if l.Port != port {
		t.Errorf("listener.Port = %d, want %d", l.Port, port)
	}
	// Listener should be bound to 127.0.0.1.
	addr := l.Addr().String()
	if !startsWith(addr, "127.0.0.1:") {
		t.Errorf("addr = %q, want 127.0.0.1:...", addr)
	}
	// And the port must be reachable.
	conn, err := net.Dial("tcp", addr)
	if err != nil {
		t.Fatalf("dial: %v", err)
	}
	conn.Close()
}

func TestAllocatePort_SpecificPort(t *testing.T) {
	// Pick a free port first, then ask for it.
	l0, port, err := Allocate()
	if err != nil {
		t.Fatalf("seed Allocate: %v", err)
	}
	l0.Close()

	// The port is now likely free again. Try to bind it explicitly.
	l, got, err := AllocatePort(port)
	if err != nil {
		// Race: another process grabbed it. Skip.
		t.Skipf("could not reclaim port %d: %v", port, err)
	}
	defer l.Close()
	if got != port {
		t.Errorf("got port = %d, want %d", got, port)
	}
}

func TestAllocatePort_ZeroIsAuto(t *testing.T) {
	l, port, err := AllocatePort(0)
	if err != nil {
		t.Fatal(err)
	}
	defer l.Close()
	if port == 0 {
		t.Error("port = 0, want non-zero")
	}
}

func TestAllocatePort_InUseFails(t *testing.T) {
	l, port, err := Allocate()
	if err != nil {
		t.Fatal(err)
	}
	defer l.Close()

	// Second bind to same port must fail.
	_, _, err = AllocatePort(port)
	if err == nil {
		t.Fatal("expected error when binding to in-use port")
	}
}

func startsWith(s, prefix string) bool {
	return len(s) >= len(prefix) && s[:len(prefix)] == prefix
}
