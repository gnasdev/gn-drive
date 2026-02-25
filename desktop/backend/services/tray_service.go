package services

import (
	"context"
	"fmt"
	"log"
	"sync"

	"github.com/wailsapp/wails/v3/pkg/application"
)

// TrayService manages the system tray
type TrayService struct {
	app            *application.App
	tray           *application.SystemTray
	menu           *application.Menu
	boardService   *BoardService
	flowService    *FlowService
	window         application.Window
	mutex          sync.RWMutex
	initialized    bool
	iconData       []byte
	onShowCallback func() // called when restoring from tray to show in Dock
}

// Singleton instance
var trayServiceInstance *TrayService
var trayServiceOnce sync.Once

// GetTrayService returns the singleton TrayService instance
func GetTrayService() *TrayService {
	return trayServiceInstance
}

// SetTrayServiceInstance sets the singleton instance
func SetTrayServiceInstance(ts *TrayService) {
	trayServiceOnce.Do(func() {
		trayServiceInstance = ts
	})
}

// NewTrayService creates a new tray service
func NewTrayService(iconData []byte) *TrayService {
	return &TrayService{
		iconData: iconData,
	}
}

// SetApp sets the application reference
func (t *TrayService) SetApp(app *application.App) {
	t.app = app
}

// SetBoardService sets the board service reference
func (t *TrayService) SetBoardService(boardService *BoardService) {
	t.boardService = boardService
}

// SetFlowService sets the flow service reference
func (t *TrayService) SetFlowService(flowService *FlowService) {
	t.flowService = flowService
}

// SetWindow sets the main window reference
func (t *TrayService) SetWindow(window application.Window) {
	t.window = window
}

// SetOnShowCallback sets a callback invoked when the app is restored from tray
func (t *TrayService) SetOnShowCallback(cb func()) {
	t.onShowCallback = cb
}

// Initialize creates and shows the system tray
func (t *TrayService) Initialize() error {
	t.mutex.Lock()
	defer t.mutex.Unlock()

	if t.initialized {
		return nil
	}

	if t.app == nil {
		return fmt.Errorf("app not set")
	}

	// Create system tray
	t.tray = t.app.SystemTray.New()

	// Set icon
	if len(t.iconData) > 0 {
		t.tray.SetIcon(t.iconData)
	}

	t.tray.SetTooltip("NG Drive")

	// Build menu
	t.buildMenu()

	// Set click handler to show window
	t.tray.OnClick(func() {
		t.showWindow()
	})

	// Run tray
	t.tray.Run()

	t.initialized = true
	log.Printf("TrayService: System tray initialized")

	return nil
}

// RefreshMenu rebuilds the tray menu with current boards
func (t *TrayService) RefreshMenu() {
	t.mutex.Lock()
	defer t.mutex.Unlock()

	if !t.initialized || t.tray == nil {
		return
	}

	t.buildMenu()
}

// buildMenu creates the menu structure (must be called with mutex held)
func (t *TrayService) buildMenu() {
	if t.menu == nil {
		// First call (during Initialize): create a new menu and set it on the tray.
		// The tray's Run() will cache the native menu pointer from this object.
		t.menu = application.NewMenu()
		t.populateMenu(t.menu)
		t.tray.SetMenu(t.menu)
	} else {
		// Subsequent calls: clear and repopulate the same Menu object,
		// then call Update() so the native menu is rebuilt in-place.
		// This works around a Wails v3 issue where SetMenu() on macOS
		// stores the Go reference but never refreshes the cached native nsMenu pointer.
		t.menu.Clear()
		t.populateMenu(t.menu)
		t.menu.Update()
	}
}

// populateMenu adds flow items and standard items to the given menu
func (t *TrayService) populateMenu(menu *application.Menu) {
	// Add flows section
	if t.flowService != nil {
		flows, err := t.flowService.GetFlows(context.Background())
		if err == nil && len(flows) > 0 {
			for _, flow := range flows {
				flowId := flow.Id
				flowName := flow.Name
				if flowName == "" {
					flowName = "(Unnamed Flow)"
				}

				menu.Add(flowName).OnClick(func(ctx *application.Context) {
					t.executeFlow(flowId)
				})
			}

			menu.AddSeparator()
		}
	}

	// Add standard items
	menu.Add("Open NG Drive").OnClick(func(ctx *application.Context) {
		t.showWindow()
	})

	menu.AddSeparator()

	menu.Add("Quit").OnClick(func(ctx *application.Context) {
		t.quit()
	})
}

// executeFlow emits an event for the frontend to execute a flow
func (t *TrayService) executeFlow(flowId string) {
	log.Printf("TrayService: Requesting flow execution %s", flowId)
	if t.app != nil {
		t.app.Event.Emit("tray:execute_flow", flowId)
	}
}

// showWindow shows and focuses the main window
func (t *TrayService) showWindow() {
	if t.window != nil {
		if t.onShowCallback != nil {
			t.onShowCallback()
		}
		t.window.Show()
		t.window.Focus()
	}
}

// quit exits the application
func (t *TrayService) quit() {
	if t.app != nil {
		t.app.Quit()
	}
}
