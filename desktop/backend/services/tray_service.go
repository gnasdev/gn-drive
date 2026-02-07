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
	app          *application.App
	tray         *application.SystemTray
	menu         *application.Menu
	boardService *BoardService
	window       application.Window
	mutex        sync.RWMutex
	initialized  bool
	iconData     []byte
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

// SetWindow sets the main window reference
func (t *TrayService) SetWindow(window application.Window) {
	t.window = window
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

	t.tray.SetTooltip("NS-Drive")

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
	menu := application.NewMenu()

	// Add boards section
	if t.boardService != nil {
		boards, err := t.boardService.GetBoards(context.Background())
		if err == nil && len(boards) > 0 {
			for _, board := range boards {
				boardId := board.Id
				boardName := board.Name
				if boardName == "" {
					boardName = "(Unnamed)"
				}

				menu.Add(boardName).OnClick(func(ctx *application.Context) {
					t.executeBoard(boardId)
				})
			}

			menu.AddSeparator()
		}
	}

	// Add standard items
	menu.Add("Open NS-Drive").OnClick(func(ctx *application.Context) {
		t.showWindow()
	})

	menu.AddSeparator()

	menu.Add("Quit").OnClick(func(ctx *application.Context) {
		t.quit()
	})

	t.tray.SetMenu(menu)
	t.menu = menu
}

// executeBoard starts execution of a board
func (t *TrayService) executeBoard(boardId string) {
	if t.boardService == nil {
		log.Printf("TrayService: Board service not available")
		return
	}

	log.Printf("TrayService: Executing board %s", boardId)

	go func() {
		_, err := t.boardService.ExecuteBoard(context.Background(), boardId)
		if err != nil {
			log.Printf("TrayService: Failed to execute board %s: %v", boardId, err)
		}
	}()
}

// showWindow shows and focuses the main window
func (t *TrayService) showWindow() {
	if t.window != nil {
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
