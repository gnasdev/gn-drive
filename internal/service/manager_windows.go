//go:build windows

package service

func newPlatformManager() (Manager, error) {
	return &SCMManager{}, nil
}
