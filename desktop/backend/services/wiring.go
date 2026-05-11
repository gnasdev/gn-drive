package services

import (
	"context"
	beConfig "desktop/backend/config"
	appEvents "desktop/backend/events"

	"github.com/wailsapp/wails/v3/pkg/application"
)

// AttachApp wires the Wails application reference into services without making
// the dependency setter part of the generated frontend API surface.
func AttachApp(app *application.App, targets ...interface{}) {
	for _, target := range targets {
		switch service := target.(type) {
		case *LogService:
			service.setApp(app)
		case *AuthService:
			service.setApp(app)
		case *SyncService:
			service.setApp(app)
		case *ConfigService:
			service.setApp(app)
		case *RemoteService:
			service.setApp(app)
		case *TabService:
			service.setApp(app)
		case *OperationService:
			service.setApp(app)
		case *HistoryService:
			service.setApp(app)
		case *SchedulerService:
			service.setApp(app)
		case *NotificationService:
			service.setApp(app)
		case *CryptService:
			service.setApp(app)
		case *BoardService:
			service.setApp(app)
		case *ExportService:
			service.setApp(app)
		case *ImportService:
			service.setApp(app)
		case *FlowService:
			service.setApp(app)
		case *TrayService:
			service.setApp(app)
		}
	}
}

// ConfigureAuthService wires auth dependencies for deferred app initialization.
func ConfigureAuthService(
	authService *AuthService,
	appInitializer func(context.Context) error,
	notificationService *NotificationService,
) {
	authService.setAppInitializer(appInitializer)
	authService.setNotificationService(notificationService)
}

// GetPreUnlockSettings returns app settings from auth.json before encrypted DB access.
func GetPreUnlockSettings(authService *AuthService) AppSettings {
	return authService.getPreUnlockSettings()
}

// ApplyPreUnlockSettings seeds app settings that are needed before encrypted DB access.
func ApplyPreUnlockSettings(notificationService *NotificationService, settings AppSettings) {
	notificationService.applyPreUnlockSettings(settings)
}

// ConfigureSyncService wires runtime-only sync dependencies.
func ConfigureSyncService(
	syncService *SyncService,
	envConfig beConfig.Config,
	logService *LogService,
	notificationService *NotificationService,
) {
	syncService.setEnvConfig(envConfig)
	syncService.setLogService(logService)
	syncService.setNotificationService(notificationService)
}

// ConfigureSchedulerService wires the scheduler to sync execution.
func ConfigureSchedulerService(schedulerService *SchedulerService, syncService *SyncService) {
	schedulerService.setSyncService(syncService)
}

// ConfigureBoardService wires board execution dependencies.
func ConfigureBoardService(
	boardService *BoardService,
	syncService *SyncService,
	notificationService *NotificationService,
) {
	boardService.setSyncService(syncService)
	boardService.setNotificationService(notificationService)
}

// ConfigureLogService wires the shared event bus into log delivery.
func ConfigureLogService(logService *LogService, eventBus *appEvents.WailsEventBus) {
	logService.setEventBus(eventBus)
}
