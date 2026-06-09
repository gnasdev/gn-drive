// Package ports provides TCP port allocation helpers.
package ports

import (
	"fmt"
	"net"
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
// Fails if the requested port is already in use.
func AllocatePort(port int) (*Listener, int, error) {
	addr := fmt.Sprintf("127.0.0.1:%d", port)
	l, err := net.Listen("tcp", addr)
	if err != nil {
		return nil, 0, err
	}
	tcp := l.Addr().(*net.TCPAddr)
	return &Listener{Listener: l, Port: tcp.Port}, tcp.Port, nil
}
