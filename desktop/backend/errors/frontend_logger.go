package errors

import (
	"desktop/backend/models"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"sync"
	"time"
)

// FrontendLogger handles logging of frontend messages
type FrontendLogger struct {
	logger     *log.Logger
	file       *os.File
	mutex      sync.Mutex
	debug      bool
	logPath    string
	maxLogSize int64 // Maximum log file size in bytes
}

// NewFrontendLogger creates a new frontend logger
func NewFrontendLogger(debug bool) *FrontendLogger {
	logPath := getFrontendLogFile()
	file, err := os.OpenFile(logPath, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)
	if err != nil {
		log.Printf("Failed to open frontend log file: %v", err)
		file = os.Stderr
	}

	logger := log.New(file, "[FRONTEND] ", log.LstdFlags)

	return &FrontendLogger{
		logger:     logger,
		file:       file,
		debug:      debug,
		logPath:    logPath,
		maxLogSize: 50 * 1024 * 1024, // 50MB max log file size
	}
}

// getFrontendLogFile returns the path for frontend log file
func getFrontendLogFile() string {
	wd, err := os.Getwd()
	if err != nil {
		log.Printf("Failed to get working directory: %v", err)
		return "ng-drive-frontend.log"
	}
	return filepath.Join(wd, "ng-drive-frontend.log")
}

// LogEntry logs a frontend log entry
func (fl *FrontendLogger) LogEntry(entry *models.FrontendLogEntry) error {
	fl.mutex.Lock()
	defer fl.mutex.Unlock()

	if entry == nil || !entry.IsValid() {
		return fmt.Errorf("invalid log entry")
	}

	// Only log WARN, ERROR, and CRITICAL levels
	if !fl.shouldLog(entry.Level) {
		// Still log to console in debug mode for all levels
		if fl.debug {
			fmt.Printf("[FRONTEND-%s] %s (NOT LOGGED TO FILE)\n", entry.Level, fl.formatLogMessage(entry))
		}
		return nil
	}

	// Check if log rotation is needed
	if err := fl.rotateLogIfNeeded(); err != nil {
		log.Printf("Failed to rotate log file: %v", err)
	}

	// Format the log message
	logMessage := fl.formatLogMessage(entry)

	// Write to log file
	fl.logger.Print(logMessage)

	// Also log to console in debug mode
	if fl.debug {
		fmt.Printf("[FRONTEND-%s] %s\n", entry.Level, logMessage)
	}

	return nil
}

// shouldLog determines if a log level should be written to file
func (fl *FrontendLogger) shouldLog(level models.LogLevel) bool {
	// Only log WARN, ERROR, and CRITICAL levels to file
	return level == models.WARN || level == models.ERROR || level == models.CRITICAL
}

// formatLogMessage formats a log entry into a readable string
func (fl *FrontendLogger) formatLogMessage(entry *models.FrontendLogEntry) string {
	message := fmt.Sprintf("Level: %s | Message: %s", entry.Level, entry.Message)

	if entry.Context != "" {
		message += fmt.Sprintf(" | Context: %s", entry.Context)
	}

	if entry.Component != "" {
		message += fmt.Sprintf(" | Component: %s", entry.Component)
	}

	if entry.URL != "" {
		message += fmt.Sprintf(" | URL: %s", entry.URL)
	}

	if entry.TraceID != "" {
		message += fmt.Sprintf(" | TraceID: %s", entry.TraceID)
	}

	if entry.Details != "" {
		message += fmt.Sprintf(" | Details: %s", entry.Details)
	}

	if entry.UserAgent != "" {
		message += fmt.Sprintf(" | UserAgent: %s", entry.UserAgent)
	}

	if entry.BrowserInfo != "" {
		message += fmt.Sprintf(" | BrowserInfo: %s", entry.BrowserInfo)
	}

	if entry.StackTrace != "" {
		message += fmt.Sprintf(" | StackTrace: %s", entry.StackTrace)
	}

	return message
}

// rotateLogIfNeeded rotates the log file if it exceeds the maximum size
func (fl *FrontendLogger) rotateLogIfNeeded() error {
	if fl.file == os.Stderr {
		return nil // Don't rotate stderr
	}

	fileInfo, err := fl.file.Stat()
	if err != nil {
		return err
	}

	if fileInfo.Size() > fl.maxLogSize {
		// Close current file
		if err := fl.file.Close(); err != nil {
			log.Printf("Failed to close log file: %v", err)
		}

		// Rename current log file with timestamp
		timestamp := time.Now().Format("20060102-150405")
		backupPath := fmt.Sprintf("%s.%s", fl.logPath, timestamp)
		if err := os.Rename(fl.logPath, backupPath); err != nil {
			log.Printf("Failed to backup log file: %v", err)
		}

		// Create new log file
		newFile, err := os.OpenFile(fl.logPath, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)
		if err != nil {
			return err
		}

		fl.file = newFile
		fl.logger = log.New(newFile, "[FRONTEND] ", log.LstdFlags)
	}

	return nil
}

// LogInfo logs an info level message
func (fl *FrontendLogger) LogInfo(message, context string) {
	entry := &models.FrontendLogEntry{
		Level:     models.INFO,
		Message:   message,
		Context:   context,
		Timestamp: time.Now(),
	}
	if err := fl.LogEntry(entry); err != nil {
		log.Printf("Failed to log warning entry: %v", err)
	}
}

// LogWarning logs a warning level message
func (fl *FrontendLogger) LogWarning(message, context string) {
	entry := &models.FrontendLogEntry{
		Level:     models.WARN,
		Message:   message,
		Context:   context,
		Timestamp: time.Now(),
	}
	if err := fl.LogEntry(entry); err != nil {
		log.Printf("Failed to log error entry: %v", err)
	}
}

// LogError logs an error level message
func (fl *FrontendLogger) LogError(message, context string) {
	entry := &models.FrontendLogEntry{
		Level:     models.ERROR,
		Message:   message,
		Context:   context,
		Timestamp: time.Now(),
	}
	if err := fl.LogEntry(entry); err != nil {
		log.Printf("Failed to log critical entry: %v", err)
	}
}

// LogCritical logs a critical level message
func (fl *FrontendLogger) LogCritical(message, context string) {
	entry := &models.FrontendLogEntry{
		Level:     models.CRITICAL,
		Message:   message,
		Context:   context,
		Timestamp: time.Now(),
	}
	if err := fl.LogEntry(entry); err != nil {
		log.Printf("Failed to log critical entry: %v", err)
	}
}

// Close closes the log file
func (fl *FrontendLogger) Close() error {
	fl.mutex.Lock()
	defer fl.mutex.Unlock()

	if fl.file != nil && fl.file != os.Stderr {
		return fl.file.Close()
	}
	return nil
}
