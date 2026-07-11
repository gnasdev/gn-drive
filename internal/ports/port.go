// Package ports provides TCP port allocation helpers.
package ports

import (
	"fmt"
	"net"
	"time"
)

// DefaultPort is the fixed loopback port for the gn-drive web UI + API.
//
// Chosen in the IANA dynamic/private range (49152–65535) to avoid collisions
// with well-known services and common dev servers (3000, 5173, 8080, 5432, …).
// Keep in sync with: .air.toml, scripts/dev.sh, frontend/vite.config.ts proxy.
const DefaultPort = 53241

// Listener is a net.Listener with explicit port tracking.
type Listener struct {
	net.Listener
	Port int
}

// Allocate binds DefaultPort on 127.0.0.1.
// Deprecated name kept for tests/callers; prefer AllocatePort(DefaultPort).
func Allocate() (*Listener, int, error) {
	return AllocatePort(DefaultPort)
}

// AllocatePort binds a TCP listener on 127.0.0.1:port.
// Port 0 is treated as DefaultPort (no kernel auto-assign).
//
// When binding, a few short retries are used so hot reload (air) can rebind
// the same port while the previous process is still releasing the socket.
func AllocatePort(port int) (*Listener, int, error) {
	if port == 0 {
		port = DefaultPort
	}
	addr := fmt.Sprintf("127.0.0.1:%d", port)
	attempts := 20
	var lastErr error
	for i := 0; i < attempts; i++ {
		l, err := net.Listen("tcp", addr)
		if err == nil {
			tcp := l.Addr().(*net.TCPAddr)
			return &Listener{Listener: l, Port: tcp.Port}, tcp.Port, nil
		}
		lastErr = err
		time.Sleep(50 * time.Millisecond)
	}
	return nil, 0, fmt.Errorf("bind %s: %w", addr, lastErr)
}
