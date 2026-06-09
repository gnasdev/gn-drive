//go:build darwin

package service

func newPlatformManager() (Manager, error) {
	return &LaunchdManager{}, nil
}
