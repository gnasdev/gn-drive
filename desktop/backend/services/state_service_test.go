package services

import (
	"context"
	"desktop/backend/models"
	"testing"

	fsConfig "github.com/rclone/rclone/fs/config"
)

type fakeStateProvider struct {
	configInfo models.ConfigInfo
	remotes    []fsConfig.Remote
}

func (f fakeStateProvider) GetConfigInfo() models.ConfigInfo {
	return f.configInfo
}

func (f fakeStateProvider) GetRemotes() []fsConfig.Remote {
	return f.remotes
}

func TestStateService_GetAppStateDefaultsEmptySlices(t *testing.T) {
	service := NewStateService(nil, fakeStateProvider{})

	state, err := service.GetAppState(context.Background())
	if err != nil {
		t.Fatalf("GetAppState returned error: %v", err)
	}

	if state.ConfigInfo.Profiles == nil {
		t.Fatal("expected profiles to be an empty slice, got nil")
	}
	if state.Remotes == nil {
		t.Fatal("expected remotes to be an empty slice, got nil")
	}
}

func TestStateService_GetAppStateProjectsProviderData(t *testing.T) {
	profile := models.Profile{Name: "daily", From: "local:/a", To: "drive:/b"}
	remote := fsConfig.Remote{Name: "drive", Type: "drive"}
	service := NewStateService(nil, fakeStateProvider{
		configInfo: models.ConfigInfo{Profiles: []models.Profile{profile}},
		remotes:    []fsConfig.Remote{remote},
	})

	state, err := service.GetAppState(context.Background())
	if err != nil {
		t.Fatalf("GetAppState returned error: %v", err)
	}

	if got := len(state.ConfigInfo.Profiles); got != 1 {
		t.Fatalf("expected 1 profile, got %d", got)
	}
	if got := state.ConfigInfo.Profiles[0].Name; got != profile.Name {
		t.Fatalf("expected profile %q, got %q", profile.Name, got)
	}
	if got := len(state.Remotes); got != 1 {
		t.Fatalf("expected 1 remote, got %d", got)
	}
	if got := state.Remotes[0].Name; got != remote.Name {
		t.Fatalf("expected remote %q, got %q", remote.Name, got)
	}
}
