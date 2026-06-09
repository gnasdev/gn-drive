//go:build !linux && !darwin && !windows

package service

import (
	"fmt"
	"runtime"
)

func newPlatformManager() (Manager, error) {
	return nil, fmt.Errorf("%w: %s", ErrNotSupported, runtime.GOOS)
}
