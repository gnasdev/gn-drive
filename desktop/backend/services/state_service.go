package services

// GN Drive note: Owns application state snapshots that the frontend renders.

import (
	"context"
	appEvents "desktop/backend/events"
	"desktop/backend/models"
	"log"
	"sync"

	fsConfig "github.com/rclone/rclone/fs/config"
	"github.com/wailsapp/wails/v3/pkg/application"
)

// LegacyStateProvider lets StateService project state from the current backend
// owners while the legacy App bridge is still being phased out.
type LegacyStateProvider interface {
	GetConfigInfo() models.ConfigInfo
	GetRemotes() []fsConfig.Remote
}

// AppState is the backend-owned snapshot rendered by the frontend.
type AppState struct {
	ConfigInfo models.ConfigInfo `json:"configInfo"`
	Remotes    []fsConfig.Remote `json:"remotes"`
	Version    uint64            `json:"version"`
}

// StateService exposes canonical app state snapshots and patch events.
type StateService struct {
	app      *application.App
	eventBus *appEvents.WailsEventBus
	provider LegacyStateProvider
	mutex    sync.Mutex
	seqNo    uint64
}

// NewStateService creates the backend-owned state projection service.
func NewStateService(app *application.App, provider LegacyStateProvider) *StateService {
	return &StateService{
		app:      app,
		provider: provider,
	}
}

func (s *StateService) setApp(app *application.App) {
	s.app = app
	if bus := GetSharedEventBus(); bus != nil {
		s.eventBus = bus
	} else {
		s.eventBus = appEvents.NewEventBus(app)
	}
}

func (s *StateService) ServiceName() string {
	return "StateService"
}

func (s *StateService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("StateService starting up...")
	return nil
}

func (s *StateService) ServiceShutdown(ctx context.Context) error {
	log.Printf("StateService shutting down...")
	return nil
}

// GetAppState returns a full backend-owned state snapshot.
func (s *StateService) GetAppState(ctx context.Context) (AppState, error) {
	state := AppState{}
	if s.provider == nil {
		return state, nil
	}

	state.ConfigInfo = s.provider.GetConfigInfo()
	state.Remotes = s.provider.GetRemotes()
	state.Version = s.currentSeqNo()
	if state.ConfigInfo.Profiles == nil {
		state.ConfigInfo.Profiles = []models.Profile{}
	}
	if state.Remotes == nil {
		state.Remotes = []fsConfig.Remote{}
	}
	return state, nil
}

// emitSnapshot publishes the full state for frontend recovery.
func (s *StateService) emitSnapshot(ctx context.Context) error {
	state, err := s.GetAppState(ctx)
	if err != nil {
		return err
	}
	return s.emitState(appEvents.StateSnapshot, "", "replace", state)
}

// emitConfigPatch publishes the canonical config slice after backend changes.
func (s *StateService) emitConfigPatch(ctx context.Context) error {
	state, err := s.GetAppState(ctx)
	if err != nil {
		return err
	}
	return s.emitState(appEvents.StatePatch, "config", "replace", state.ConfigInfo)
}

// emitRemotesPatch publishes the canonical remotes slice after backend changes.
func (s *StateService) emitRemotesPatch(ctx context.Context) error {
	state, err := s.GetAppState(ctx)
	if err != nil {
		return err
	}
	return s.emitState(appEvents.StatePatch, "remotes", "replace", state.Remotes)
}

func (s *StateService) currentSeqNo() uint64 {
	s.mutex.Lock()
	defer s.mutex.Unlock()
	return s.seqNo
}

func (s *StateService) nextSeqNo() uint64 {
	s.mutex.Lock()
	defer s.mutex.Unlock()
	s.seqNo++
	return s.seqNo
}

func (s *StateService) emitState(eventType appEvents.EventType, slice, operation string, data interface{}) error {
	event := appEvents.NewStateEvent(eventType, s.nextSeqNo(), slice, operation, data)
	if s.eventBus != nil {
		if err := s.eventBus.EmitStateEvent(event); err != nil {
			return err
		}
		return nil
	}
	if s.app != nil {
		s.app.Event.Emit("tofe", event)
		return nil
	}
	return nil
}
