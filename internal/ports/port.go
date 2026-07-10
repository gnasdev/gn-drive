// Package ports provides TCP port allocation helpers.
package ports

import (
	"fmt"
	"net"
	"time"
)

// Listener is a net.Listener with explicit port tracking.
type Listener struct {
	net.Listener
	Port int
}

// Allocate binds a TCP listener on 127.0.0.1 with port 0 (kernel picks)
// and returns the listener plus the assigned port number. The caller must
// close the listener when done.
func Allocate() (*Listener, int, error) {
	return AllocatePort(0)
}

// AllocatePort binds a TCP listener on 127.0.0.1:port. If port is 0, the
// kernel picks a free port. Returns the listener and the actual bound port.
//
// When a non-zero port is requested, a few short retries are used so hot
// reload (air) can rebind the same port while the previous process is still
// releasing the socket.
func AllocatePort(port int) (*Listener, int, error) {
	addr := fmt.Sprintf("127.0.0.1:%d", port)
	attempts := 1
	if port != 0 {
		attempts = 20
	}
	var lastErr error
	for i := 0; i < attempts; i++ {
		l, err := net.Listen("tcp", addr)
		if err == nil {
			tcp := l.Addr().(*net.TCPAddr)
			return &Listener{Listener: l, Port: tcp.Port}, tcp.Port, nil
		}
		lastErr = err
		if port == 0 {
			break
		}
		time.Sleep(50 * time.Millisecond)
	}
	return nil, 0, lastErr
}
