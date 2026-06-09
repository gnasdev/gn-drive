//go:build linux

package service

import "fmt"

func newPlatformManager() (Manager, error) {
	return &SystemdManager{}, nil
}

var _ = fmt.Sprintf
