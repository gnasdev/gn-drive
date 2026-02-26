package services

import (
	"bytes"
	"compress/gzip"
	"context"
	"desktop/backend/models"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"hash/crc32"
	"io"
	"log"
	"os"
	"path/filepath"
	"sync"
	"time"

	fsConfig "github.com/rclone/rclone/fs/config"
	"github.com/wailsapp/wails/v3/pkg/application"
)

// Binary format constants
const (
	MagicBytes      = "NSDRIVE"
	FormatVersion   = uint8(1)
	SectionBoards   = uint8(0x01)
	SectionRemotes  = uint8(0x02)
	SectionSettings = uint8(0x03)
	SectionManifest = uint8(0x04)
	EOFMarker       = uint8(0xFF)
)

// Export flags
const (
	FlagCompressed    = uint32(1 << 0)
	FlagEncrypted     = uint32(1 << 1) // Reserved for future use
	FlagExcludeTokens = uint32(1 << 2)
)

// ExportService handles exporting configuration data
type ExportService struct {
	app   *application.App
	mutex sync.RWMutex
}

// ExportOptions configures what to export
type ExportOptions struct {
	IncludeBoards   bool   `json:"include_boards"`
	IncludeRemotes  bool   `json:"include_remotes"`
	IncludeSettings bool   `json:"include_settings"`
	ExcludeTokens   bool   `json:"exclude_tokens"`    // Export remotes without sensitive tokens
	EncryptPassword string `json:"encrypt_password"`   // If set, encrypt the export file
}

// ExportManifest contains metadata about the export
type ExportManifest struct {
	Version     string    `json:"version"`
	AppVersion  string    `json:"app_version"`
	ExportDate  time.Time `json:"export_date"`
	BoardCount  int       `json:"board_count"`
	RemoteCount int       `json:"remote_count"`
	Checksum    uint32    `json:"checksum"`
}

// RemoteExport represents a remote for export (may exclude sensitive data)
type RemoteExport struct {
	Name   string            `json:"name"`
	Type   string            `json:"type"`
	Config map[string]string `json:"config"`
}

// ExportData contains all exportable data
type ExportData struct {
	Manifest ExportManifest `json:"manifest"`
	Boards   []models.Board `json:"boards,omitempty"`
	Remotes  []RemoteExport `json:"remotes,omitempty"`
	Settings map[string]any `json:"settings,omitempty"`
}

// NewExportService creates a new export service
func NewExportService(app *application.App) *ExportService {
	return &ExportService{
		app: app,
	}
}

// SetApp sets the application reference
func (e *ExportService) SetApp(app *application.App) {
	e.app = app
}

// ServiceName returns the name of the service
func (e *ExportService) ServiceName() string {
	return "ExportService"
}

// ServiceStartup is called when the service starts
func (e *ExportService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("ExportService starting up...")
	return nil
}

// ServiceShutdown is called when the service shuts down
func (e *ExportService) ServiceShutdown(ctx context.Context) error {
	log.Printf("ExportService shutting down...")
	return nil
}

// GetExportPreview returns a preview of what will be exported
func (e *ExportService) GetExportPreview(ctx context.Context, options ExportOptions) (*ExportManifest, error) {
	manifest := &ExportManifest{
		Version:    fmt.Sprintf("%d", FormatVersion),
		AppVersion: "1.0.0",
		ExportDate: time.Now(),
	}

	if options.IncludeBoards {
		if boardService := GetBoardService(); boardService != nil {
			boards, err := boardService.GetBoards(ctx)
			if err == nil {
				manifest.BoardCount = len(boards)
			}
		}
	}

	if options.IncludeRemotes {
		remotes := fsConfig.GetRemotes()
		manifest.RemoteCount = len(remotes)
	}

	return manifest, nil
}

// ExportToBytes exports configuration to binary format
func (e *ExportService) ExportToBytes(ctx context.Context, options ExportOptions) ([]byte, error) {
	e.mutex.Lock()
	defer e.mutex.Unlock()

	var buf bytes.Buffer

	// Build flags
	flags := FlagCompressed
	if options.ExcludeTokens {
		flags |= FlagExcludeTokens
	}

	// Derive encryption key if password provided
	var encKey []byte
	var encSalt []byte
	if options.EncryptPassword != "" {
		flags |= FlagEncrypted
		encKey, encSalt = DeriveExportKey(options.EncryptPassword)
		defer zeroBytes(encKey)
	}

	// Collect data sections
	var sections []struct {
		sectionType uint8
		data        []byte
	}

	// Export boards
	if options.IncludeBoards {
		if boardService := GetBoardService(); boardService != nil {
			boards, err := boardService.GetBoards(ctx)
			if err != nil {
				return nil, fmt.Errorf("failed to get boards: %w", err)
			}
			if len(boards) > 0 {
				boardsJSON, err := json.Marshal(boards)
				if err != nil {
					return nil, fmt.Errorf("failed to marshal boards: %w", err)
				}
				sectionData, err := compressData(boardsJSON)
				if err != nil {
					return nil, fmt.Errorf("failed to compress boards: %w", err)
				}
				if encKey != nil {
					sectionData, err = EncryptData(sectionData, encKey)
					if err != nil {
						return nil, fmt.Errorf("failed to encrypt boards: %w", err)
					}
				}
				sections = append(sections, struct {
					sectionType uint8
					data        []byte
				}{SectionBoards, sectionData})
			}
		}
	}

	// Export remotes
	if options.IncludeRemotes {
		remotes := e.getRemotesForExport(options.ExcludeTokens)
		if len(remotes) > 0 {
			remotesJSON, err := json.Marshal(remotes)
			if err != nil {
				return nil, fmt.Errorf("failed to marshal remotes: %w", err)
			}
			sectionData, err := compressData(remotesJSON)
			if err != nil {
				return nil, fmt.Errorf("failed to compress remotes: %w", err)
			}
			if encKey != nil {
				sectionData, err = EncryptData(sectionData, encKey)
				if err != nil {
					return nil, fmt.Errorf("failed to encrypt remotes: %w", err)
				}
			}
			sections = append(sections, struct {
				sectionType uint8
				data        []byte
			}{SectionRemotes, sectionData})
		}
	}

	// Create manifest
	manifest := ExportManifest{
		Version:    fmt.Sprintf("%d", FormatVersion),
		AppVersion: "1.0.0",
		ExportDate: time.Now(),
	}
	if options.IncludeBoards {
		if boardService := GetBoardService(); boardService != nil {
			boards, _ := boardService.GetBoards(ctx)
			manifest.BoardCount = len(boards)
		}
	}
	if options.IncludeRemotes {
		manifest.RemoteCount = len(fsConfig.GetRemotes())
	}

	manifestJSON, err := json.Marshal(manifest)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal manifest: %w", err)
	}
	manifestData, err := compressData(manifestJSON)
	if err != nil {
		return nil, fmt.Errorf("failed to compress manifest: %w", err)
	}
	if encKey != nil {
		manifestData, err = EncryptData(manifestData, encKey)
		if err != nil {
			return nil, fmt.Errorf("failed to encrypt manifest: %w", err)
		}
	}
	sections = append(sections, struct {
		sectionType uint8
		data        []byte
	}{SectionManifest, manifestData})

	// Calculate checksum of all section data
	var allData bytes.Buffer
	for _, section := range sections {
		allData.Write(section.data)
	}
	checksum := crc32.ChecksumIEEE(allData.Bytes())

	// Write header
	// Magic bytes (7 bytes)
	buf.WriteString(MagicBytes)
	// Version (1 byte)
	buf.WriteByte(FormatVersion)
	// Flags (4 bytes)
	if err := binary.Write(&buf, binary.LittleEndian, flags); err != nil {
		return nil, fmt.Errorf("failed to write flags: %w", err)
	}
	// Checksum (4 bytes)
	if err := binary.Write(&buf, binary.LittleEndian, checksum); err != nil {
		return nil, fmt.Errorf("failed to write checksum: %w", err)
	}
	// Reserved (16 bytes) - stores encryption salt if encrypted
	reserved := make([]byte, 16)
	if encSalt != nil && len(encSalt) >= 16 {
		copy(reserved, encSalt[:16])
	}
	buf.Write(reserved)

	// Write sections
	for _, section := range sections {
		// Section type (1 byte)
		buf.WriteByte(section.sectionType)
		// Section length (4 bytes)
		if err := binary.Write(&buf, binary.LittleEndian, uint32(len(section.data))); err != nil {
			return nil, fmt.Errorf("failed to write section length: %w", err)
		}
		// Section data
		buf.Write(section.data)
	}

	// EOF marker
	buf.WriteByte(EOFMarker)

	log.Printf("ExportService: Exported %d sections, total size: %d bytes", len(sections), buf.Len())
	return buf.Bytes(), nil
}

// ExportToFile exports configuration to a file
func (e *ExportService) ExportToFile(ctx context.Context, filePath string, options ExportOptions) error {
	data, err := e.ExportToBytes(ctx, options)
	if err != nil {
		return err
	}

	// Ensure directory exists
	dir := filepath.Dir(filePath)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return fmt.Errorf("failed to create directory: %w", err)
	}

	if err := os.WriteFile(filePath, data, 0600); err != nil {
		return fmt.Errorf("failed to write export file: %w", err)
	}

	log.Printf("ExportService: Exported to %s (%d bytes)", filePath, len(data))
	return nil
}

// SelectExportFile opens a save dialog and returns the selected file path
func (e *ExportService) SelectExportFile(ctx context.Context) (string, error) {
	if e.app == nil {
		return "", fmt.Errorf("application not initialized")
	}

	defaultName := fmt.Sprintf("gn-drive-backup-%s.nsd", time.Now().Format("2006-01-02"))

	filePath, err := e.app.Dialog.SaveFile().
		SetMessage("Export Backup").
		SetFilename(defaultName).
		AddFilter("GN Drive Backup", "*.nsd").
		AddFilter("All Files", "*.*").
		PromptForSingleSelection()
	if err != nil {
		return "", fmt.Errorf("dialog error: %w", err)
	}

	if filePath == "" {
		return "", nil // User cancelled
	}

	// Ensure .nsd extension
	if filepath.Ext(filePath) != ".nsd" {
		filePath += ".nsd"
	}

	return filePath, nil
}

// ExportWithDialog opens a save dialog and exports to the selected file
func (e *ExportService) ExportWithDialog(ctx context.Context, options ExportOptions) (string, error) {
	filePath, err := e.SelectExportFile(ctx)
	if err != nil {
		return "", err
	}
	if filePath == "" {
		return "", nil // User cancelled
	}

	if err := e.ExportToFile(ctx, filePath, options); err != nil {
		return "", err
	}

	return filePath, nil
}

// getRemotesForExport gets remotes formatted for export
func (e *ExportService) getRemotesForExport(excludeTokens bool) []RemoteExport {
	rcloneRemotes := fsConfig.GetRemotes()
	remotes := make([]RemoteExport, 0, len(rcloneRemotes))

	// Sensitive keys to exclude
	sensitiveKeys := map[string]bool{
		"token":                true,
		"client_id":            true,
		"client_secret":        true,
		"password":             true,
		"password2":            true,
		"pass":                 true,
		"secret":               true,
		"key":                  true,
		"private_key":          true,
		"service_account_file": true,
	}

	for _, remote := range rcloneRemotes {
		export := RemoteExport{
			Name:   remote.Name,
			Type:   remote.Type,
			Config: make(map[string]string),
		}

		// Get config for this remote using DumpRcRemote
		config := fsConfig.DumpRcRemote(remote.Name)
		for key, value := range config {
			// Skip type field as it's already captured
			if key == "type" {
				continue
			}

			valueStr := fmt.Sprintf("%v", value)

			// Skip sensitive keys if excludeTokens is true
			if excludeTokens && sensitiveKeys[key] {
				continue
			}

			export.Config[key] = valueStr
		}

		remotes = append(remotes, export)
	}

	return remotes
}

// compressData compresses data using gzip
func compressData(data []byte) ([]byte, error) {
	var buf bytes.Buffer
	writer := gzip.NewWriter(&buf)
	if _, err := writer.Write(data); err != nil {
		return nil, err
	}
	if err := writer.Close(); err != nil {
		return nil, err
	}
	return buf.Bytes(), nil
}

// decompressData decompresses gzip data
func decompressData(data []byte) ([]byte, error) {
	reader, err := gzip.NewReader(bytes.NewReader(data))
	if err != nil {
		return nil, err
	}
	defer reader.Close()
	return io.ReadAll(reader)
}
