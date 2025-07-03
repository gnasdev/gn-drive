package utils

import (
	"context"
	"log"
	"os"
	"path/filepath"
	"runtime"
	"runtime/debug"
	"sync"

	"desktop/backend/constants"
)

func GetPlatform(ctx context.Context) string {
	return runtime.GOOS // windows, darwin, linux
}

func GetCurrentEnvName(ctx context.Context) string {
	// Check if we're in development mode by looking for debug info
	if info, ok := debug.ReadBuildInfo(); ok {
		for _, setting := range info.Settings {
			if setting.Key == "-tags" && setting.Value == "dev" {
				return constants.Development.String()
			}
		}
	}
	return constants.Production.String()
}

func CdToNormalizeWorkingDir(ctx context.Context) error {
	var wd string
	var err error

	if GetCurrentEnvName(ctx) == constants.Development.String() {
		wd, err = os.Getwd()
		if err != nil {
			return err
		}
		wd = filepath.Clean(wd + "/../")
	} else {
		exePath, err := os.Executable()
		if err != nil {
			return err
		}
		wd = filepath.Dir(exePath)
		if GetPlatform(ctx) != constants.Windows.String() {
			wd = filepath.Clean(wd + "/../../../")
		}
	}

	return os.Chdir(wd)
}

var (
	cmdStore      = make(map[int]func())
	cmdStoreMutex sync.RWMutex
	tabStore      = make(map[int]string)
	tabStoreMutex sync.RWMutex
)

func AddCmd(pid int, cancel func()) {
	cmdStoreMutex.Lock()
	defer cmdStoreMutex.Unlock()
	cmdStore[pid] = cancel
}

func GetCmd(pid int) (func(), bool) {
	cmdStoreMutex.RLock()
	defer cmdStoreMutex.RUnlock()
	cancel, exists := cmdStore[pid]
	return cancel, exists
}

func RemoveCmd(pid int) {
	cmdStoreMutex.Lock()
	defer cmdStoreMutex.Unlock()
	delete(cmdStore, pid)
}

func AddTabMapping(pid int, tabId string) {
	tabStoreMutex.Lock()
	defer tabStoreMutex.Unlock()
	tabStore[pid] = tabId
}

func GetTabMapping(pid int) (string, bool) {
	tabStoreMutex.RLock()
	defer tabStoreMutex.RUnlock()
	tabId, exists := tabStore[pid]
	return tabId, exists
}

func RemoveTabMapping(pid int) {
	tabStoreMutex.Lock()
	defer tabStoreMutex.Unlock()
	delete(tabStore, pid)
}

// logError logs the error to a file on the desktop, including stack trace and error line number
func LogError(inErr error) {
	if wd, err := os.Getwd(); err == nil {
		if f, fileErr := os.OpenFile(filepath.Join(wd, "desktop.log"), os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644); fileErr == nil {
			defer f.Close()
			logger := log.New(f, "", log.LstdFlags)
			logger.Printf("Error: %v\nStack Trace:\n%s", inErr, debug.Stack())
		} else {
			log.Printf("Failed to open log file: %v", fileErr)
		}
	} else {
		log.Printf("Failed to get working directory: %v", err)
	}
}

func LogErrorAndExit(err error) {
	LogError(err)
	log.Fatal(err)
}

func HandleError(err error, msg string, onError func(err error), onClear func()) error {
	if err != nil {
		log.Printf("%s: %v", msg, err)
		debug.PrintStack()

		if onError != nil {
			onError(err)
		}
	} else {
		if onClear != nil {
			onClear()
		}
	}

	return err
}
