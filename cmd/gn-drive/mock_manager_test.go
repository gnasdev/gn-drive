package main

import (
	"errors"
	"time"

	"github.com/gnasdev/gn-drive/internal/service"
)

// mockManager is a test double for service.Manager.
type mockManager struct {
	installed      bool
	installErr     error
	uninstallErr   error
	startErr       error
	stopErr        error
	restartErr     error
	statusErr      error
	isInstalledErr error
	status         service.Status
	// Recorded actions.
	installedCalled  bool
	uninstalledCalled bool
	startedCalled   bool
	stoppedCalled   bool
	restartedCalled bool
}

func (m *mockManager) Install(spec service.Spec) error {
	m.installedCalled = true
	if m.installErr != nil {
		return m.installErr
	}
	m.installed = true
	return nil
}

func (m *mockManager) Uninstall(spec service.Spec) error {
	m.uninstalledCalled = true
	if m.uninstallErr != nil {
		return m.uninstallErr
	}
	m.installed = false
	return nil
}

func (m *mockManager) Start(spec service.Spec) error {
	m.startedCalled = true
	if m.startErr != nil {
		return m.startErr
	}
	m.status.Running = true
	m.status.PID = 12345
	return nil
}

func (m *mockManager) Stop(spec service.Spec) error {
	m.stoppedCalled = true
	if m.stopErr != nil {
		return m.stopErr
	}
	m.status.Running = false
	return nil
}

func (m *mockManager) Restart(spec service.Spec) error {
	m.restartedCalled = true
	if m.restartErr != nil {
		return m.restartErr
	}
	return nil
}

func (m *mockManager) Status(spec service.Spec) (service.Status, error) {
	if m.statusErr != nil {
		return service.Status{}, m.statusErr
	}
	st := m.status
	if st.Mode == "" {
		st.Mode = "service"
	}
	if st.Scope == "" {
		st.Scope = string(spec.Scope)
	}
	return st, nil
}

func (m *mockManager) IsInstalled(spec service.Spec) (bool, error) {
	if m.isInstalledErr != nil {
		return false, m.isInstalledErr
	}
	return m.installed, nil
}

// Compile-time check.
var _ service.Manager = (*mockManager)(nil)

// errMock is a placeholder error for tests.
var errMock = errors.New("mock error")

// healthFile creates a service.health file in dir with the given data.
func healthFile(t interface{ Helper() }, dir string, age time.Duration) {
	t.Helper()
}
