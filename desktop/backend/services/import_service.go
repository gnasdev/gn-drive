package services

import (
	"bytes"
	"context"
	"desktop/backend/models"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"hash/crc32"
	"log"
	"os"
	"sync"
	"time"

	fsConfig "github.com/rclone/rclone/fs/config"
	"github.com/rclone/rclone/fs/rc"
	"github.com/wailsapp/wails/v3/pkg/application"
)

// ImportService handles importing configuration data
type ImportService struct {
	app   *application.App
	mutex sync.RWMutex
}

// ImportOptions configures how to import
type ImportOptions struct {
	OverwriteBoards  bool   `json:"overwrite_boards"`  // Overwrite existing boards with same name
	OverwriteRemotes bool   `json:"overwrite_remotes"` // Overwrite existing remotes with same name
	MergeMode        bool   `json:"merge_mode"`        // Add new items only, skip existing
	Password         string `json:"password"`           // Password for encrypted backups
}

// ImportPreview shows what will happen during import
type ImportPreview struct {
	Valid     bool                  `json:"valid"`
	Encrypted bool                 `json:"encrypted"` // File is encrypted, needs password
	Manifest  *ExportManifest      `json:"manifest,omitempty"`
	Boards    *ImportPreviewSection `json:"boards,omitempty"`
	Remotes   *ImportPreviewSection `json:"remotes,omitempty"`
	Warnings  []string             `json:"warnings"`
	Errors    []string             `json:"errors"`
}

// ImportPreviewSection shows changes for a specific section
type ImportPreviewSection struct {
	ToAdd    []string `json:"to_add"`
	ToUpdate []string `json:"to_update"`
	ToSkip   []string `json:"to_skip"`
	Total    int      `json:"total"`
}

// ImportResult shows the result of an import operation
type ImportResult struct {
	Success        bool     `json:"success"`
	BoardsAdded    int      `json:"boards_added"`
	BoardsUpdated  int      `json:"boards_updated"`
	BoardsSkipped  int      `json:"boards_skipped"`
	RemotesAdded   int      `json:"remotes_added"`
	RemotesUpdated int      `json:"remotes_updated"`
	RemotesSkipped int      `json:"remotes_skipped"`
	Warnings       []string `json:"warnings"`
	Errors         []string `json:"errors"`
}

// parsedExport holds parsed export data
type parsedExport struct {
	manifest *ExportManifest
	boards   []models.Board
	remotes  []RemoteExport
	flags    uint32
}

// NewImportService creates a new import service
func NewImportService(app *application.App) *ImportService {
	return &ImportService{
		app: app,
	}
}

// SetApp sets the application reference
func (i *ImportService) SetApp(app *application.App) {
	i.app = app
}

// ServiceName returns the name of the service
func (i *ImportService) ServiceName() string {
	return "ImportService"
}

// ServiceStartup is called when the service starts
func (i *ImportService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("ImportService starting up...")
	return nil
}

// ServiceShutdown is called when the service shuts down
func (i *ImportService) ServiceShutdown(ctx context.Context) error {
	log.Printf("ImportService shutting down...")
	return nil
}

// ValidateImportFile validates an import file and returns a preview.
// If file is encrypted, returns preview with Encrypted=true. Use ValidateImportFileWithPassword.
func (i *ImportService) ValidateImportFile(ctx context.Context, filePath string) (*ImportPreview, error) {
	data, err := os.ReadFile(filePath)
	if err != nil {
		return nil, fmt.Errorf("failed to read file: %w", err)
	}
	return i.validateImportData(ctx, data, "")
}

// ValidateImportFileWithPassword validates an encrypted import file with a password
func (i *ImportService) ValidateImportFileWithPassword(ctx context.Context, filePath string, password string) (*ImportPreview, error) {
	data, err := os.ReadFile(filePath)
	if err != nil {
		return nil, fmt.Errorf("failed to read file: %w", err)
	}
	return i.validateImportData(ctx, data, password)
}

// validateImportData validates import data and returns a preview
func (i *ImportService) validateImportData(ctx context.Context, data []byte, password string) (*ImportPreview, error) {
	preview := &ImportPreview{
		Valid:    false,
		Warnings: []string{},
		Errors:   []string{},
	}

	// Check if file is encrypted first
	encrypted, err := isExportEncrypted(data)
	if err != nil {
		preview.Errors = append(preview.Errors, fmt.Sprintf("Invalid file format: %v", err))
		return preview, nil
	}

	if encrypted && password == "" {
		preview.Encrypted = true
		return preview, nil
	}

	parsed, err := i.parseExportData(data, password)
	if err != nil {
		preview.Errors = append(preview.Errors, fmt.Sprintf("Invalid file format: %v", err))
		return preview, nil
	}

	preview.Manifest = parsed.manifest
	preview.Valid = true
	preview.Encrypted = encrypted

	// Check if tokens were excluded
	if parsed.flags&FlagExcludeTokens != 0 {
		preview.Warnings = append(preview.Warnings, "This backup was exported without authentication tokens. Remotes will need to be re-authenticated after import.")
	}

	// Preview boards
	if len(parsed.boards) > 0 {
		preview.Boards = i.previewBoards(ctx, parsed.boards)
	}

	// Preview remotes
	if len(parsed.remotes) > 0 {
		preview.Remotes = i.previewRemotes(parsed.remotes)
	}

	return preview, nil
}

// ImportFromFile imports configuration from a file
func (i *ImportService) ImportFromFile(ctx context.Context, filePath string, options ImportOptions) (*ImportResult, error) {
	data, err := os.ReadFile(filePath)
	if err != nil {
		return nil, fmt.Errorf("failed to read file: %w", err)
	}
	return i.ImportFromBytes(ctx, data, options)
}

// ImportFromBytes imports configuration from binary data
func (i *ImportService) ImportFromBytes(ctx context.Context, data []byte, options ImportOptions) (*ImportResult, error) {
	i.mutex.Lock()
	defer i.mutex.Unlock()

	result := &ImportResult{
		Success:  true,
		Warnings: []string{},
		Errors:   []string{},
	}

	parsed, err := i.parseExportData(data, options.Password)
	if err != nil {
		return nil, fmt.Errorf("invalid file format: %w", err)
	}

	// Check if tokens were excluded
	if parsed.flags&FlagExcludeTokens != 0 {
		result.Warnings = append(result.Warnings, "Backup was exported without authentication tokens. Remotes will need re-authentication.")
	}

	// Import boards
	if len(parsed.boards) > 0 {
		boardResult := i.importBoards(ctx, parsed.boards, options)
		result.BoardsAdded = boardResult.added
		result.BoardsUpdated = boardResult.updated
		result.BoardsSkipped = boardResult.skipped
		result.Warnings = append(result.Warnings, boardResult.warnings...)
		result.Errors = append(result.Errors, boardResult.errors...)
	}

	// Import remotes
	if len(parsed.remotes) > 0 {
		remoteResult := i.importRemotes(ctx, parsed.remotes, options)
		result.RemotesAdded = remoteResult.added
		result.RemotesUpdated = remoteResult.updated
		result.RemotesSkipped = remoteResult.skipped
		result.Warnings = append(result.Warnings, remoteResult.warnings...)
		result.Errors = append(result.Errors, remoteResult.errors...)
	}

	if len(result.Errors) > 0 {
		result.Success = false
	}

	log.Printf("ImportService: Import completed - Boards: %d added, %d updated, %d skipped; Remotes: %d added, %d updated, %d skipped",
		result.BoardsAdded, result.BoardsUpdated, result.BoardsSkipped,
		result.RemotesAdded, result.RemotesUpdated, result.RemotesSkipped)

	return result, nil
}

// SelectImportFile opens a file dialog and returns the selected file path
func (i *ImportService) SelectImportFile(ctx context.Context) (string, error) {
	if i.app == nil {
		return "", fmt.Errorf("application not initialized")
	}

	filePath, err := i.app.Dialog.OpenFile().
		SetMessage("Select Backup File").
		AddFilter("NS-Drive Backup", "*.nsd").
		AddFilter("All Files", "*.*").
		PromptForSingleSelection()
	if err != nil {
		return "", fmt.Errorf("dialog error: %w", err)
	}

	return filePath, nil
}

// ImportWithDialog opens a file dialog and imports from the selected file
func (i *ImportService) ImportWithDialog(ctx context.Context, options ImportOptions) (*ImportResult, error) {
	filePath, err := i.SelectImportFile(ctx)
	if err != nil {
		return nil, err
	}
	if filePath == "" {
		return nil, nil // User cancelled
	}

	return i.ImportFromFile(ctx, filePath, options)
}

// PreviewWithDialog opens a file dialog and returns import preview
func (i *ImportService) PreviewWithDialog(ctx context.Context) (*ImportPreview, string, error) {
	filePath, err := i.SelectImportFile(ctx)
	if err != nil {
		return nil, "", err
	}
	if filePath == "" {
		return nil, "", nil // User cancelled
	}

	preview, err := i.ValidateImportFile(ctx, filePath)
	return preview, filePath, err
}

// isExportEncrypted checks if the export file header has the encrypted flag
func isExportEncrypted(data []byte) (bool, error) {
	if len(data) < 32 {
		return false, fmt.Errorf("file too small")
	}

	// Skip magic (7) + version (1) = 8 bytes
	if string(data[:7]) != MagicBytes {
		return false, fmt.Errorf("invalid file format (bad magic bytes)")
	}

	// Read flags at offset 8
	flags := binary.LittleEndian.Uint32(data[8:12])
	return flags&FlagEncrypted != 0, nil
}

// parseExportData parses binary export data, decrypting if needed
func (i *ImportService) parseExportData(data []byte, password string) (*parsedExport, error) {
	if len(data) < 32 {
		return nil, fmt.Errorf("file too small")
	}

	reader := bytes.NewReader(data)

	// Read and validate magic bytes
	magic := make([]byte, 7)
	if _, err := reader.Read(magic); err != nil {
		return nil, fmt.Errorf("failed to read magic bytes: %w", err)
	}
	if string(magic) != MagicBytes {
		return nil, fmt.Errorf("invalid file format (bad magic bytes)")
	}

	// Read version
	var version uint8
	if err := binary.Read(reader, binary.LittleEndian, &version); err != nil {
		return nil, fmt.Errorf("failed to read version: %w", err)
	}
	if version > FormatVersion {
		return nil, fmt.Errorf("unsupported format version: %d (max supported: %d)", version, FormatVersion)
	}

	// Read flags
	var flags uint32
	if err := binary.Read(reader, binary.LittleEndian, &flags); err != nil {
		return nil, fmt.Errorf("failed to read flags: %w", err)
	}

	// Read checksum
	var storedChecksum uint32
	if err := binary.Read(reader, binary.LittleEndian, &storedChecksum); err != nil {
		return nil, fmt.Errorf("failed to read checksum: %w", err)
	}

	// Read reserved bytes (contains encryption salt if encrypted)
	reserved := make([]byte, 16)
	if _, err := reader.Read(reserved); err != nil {
		return nil, fmt.Errorf("failed to read reserved bytes: %w", err)
	}

	// Derive decryption key if file is encrypted
	isEncrypted := flags&FlagEncrypted != 0
	var decKey []byte
	if isEncrypted {
		if password == "" {
			return nil, fmt.Errorf("file is encrypted but no password provided")
		}
		// Salt is stored in reserved bytes (16 bytes)
		decKey = DeriveExportKeyWithSalt(password, reserved)
		defer zeroBytes(decKey)
	}

	// Read sections
	parsed := &parsedExport{
		flags: flags,
	}

	var allSectionData bytes.Buffer

	for {
		// Read section type
		var sectionType uint8
		if err := binary.Read(reader, binary.LittleEndian, &sectionType); err != nil {
			return nil, fmt.Errorf("failed to read section type: %w", err)
		}

		// Check for EOF marker
		if sectionType == EOFMarker {
			break
		}

		// Read section length
		var sectionLen uint32
		if err := binary.Read(reader, binary.LittleEndian, &sectionLen); err != nil {
			return nil, fmt.Errorf("failed to read section length: %w", err)
		}

		// Read section data
		sectionData := make([]byte, sectionLen)
		if _, err := reader.Read(sectionData); err != nil {
			return nil, fmt.Errorf("failed to read section data: %w", err)
		}

		allSectionData.Write(sectionData)

		// Decrypt if needed
		processedData := sectionData
		if isEncrypted {
			var err error
			processedData, err = DecryptData(sectionData, decKey)
			if err != nil {
				return nil, fmt.Errorf("failed to decrypt section (wrong password?): %w", err)
			}
		}

		// Decompress if needed
		var jsonData []byte
		if flags&FlagCompressed != 0 {
			var err error
			jsonData, err = decompressData(processedData)
			if err != nil {
				return nil, fmt.Errorf("failed to decompress section: %w", err)
			}
		} else {
			jsonData = processedData
		}

		// Parse section based on type
		switch sectionType {
		case SectionBoards:
			var boards []models.Board
			if err := json.Unmarshal(jsonData, &boards); err != nil {
				return nil, fmt.Errorf("failed to parse boards: %w", err)
			}
			parsed.boards = boards

		case SectionRemotes:
			var remotes []RemoteExport
			if err := json.Unmarshal(jsonData, &remotes); err != nil {
				return nil, fmt.Errorf("failed to parse remotes: %w", err)
			}
			parsed.remotes = remotes

		case SectionManifest:
			var manifest ExportManifest
			if err := json.Unmarshal(jsonData, &manifest); err != nil {
				return nil, fmt.Errorf("failed to parse manifest: %w", err)
			}
			parsed.manifest = &manifest

		case SectionSettings:
			// Future use
		}
	}

	// Verify checksum
	calculatedChecksum := crc32.ChecksumIEEE(allSectionData.Bytes())
	if calculatedChecksum != storedChecksum {
		return nil, fmt.Errorf("checksum mismatch: file may be corrupted")
	}

	return parsed, nil
}

// previewBoards generates a preview of board import
func (i *ImportService) previewBoards(ctx context.Context, boards []models.Board) *ImportPreviewSection {
	preview := &ImportPreviewSection{
		ToAdd:    []string{},
		ToUpdate: []string{},
		ToSkip:   []string{},
		Total:    len(boards),
	}

	boardService := GetBoardService()
	if boardService == nil {
		// Can't compare, assume all are new
		for _, board := range boards {
			preview.ToAdd = append(preview.ToAdd, board.Name)
		}
		return preview
	}

	existingBoards, _ := boardService.GetBoards(ctx)
	existingMap := make(map[string]bool)
	for _, b := range existingBoards {
		existingMap[b.Name] = true
	}

	for _, board := range boards {
		if existingMap[board.Name] {
			preview.ToUpdate = append(preview.ToUpdate, board.Name)
		} else {
			preview.ToAdd = append(preview.ToAdd, board.Name)
		}
	}

	return preview
}

// previewRemotes generates a preview of remote import
func (i *ImportService) previewRemotes(remotes []RemoteExport) *ImportPreviewSection {
	preview := &ImportPreviewSection{
		ToAdd:    []string{},
		ToUpdate: []string{},
		ToSkip:   []string{},
		Total:    len(remotes),
	}

	existingRemotes := fsConfig.GetRemotes()
	existingMap := make(map[string]bool)
	for _, r := range existingRemotes {
		existingMap[r.Name] = true
	}

	for _, remote := range remotes {
		if existingMap[remote.Name] {
			preview.ToUpdate = append(preview.ToUpdate, remote.Name)
		} else {
			preview.ToAdd = append(preview.ToAdd, remote.Name)
		}
	}

	return preview
}

type importSectionResult struct {
	added    int
	updated  int
	skipped  int
	warnings []string
	errors   []string
}

// importBoards imports boards
func (i *ImportService) importBoards(ctx context.Context, boards []models.Board, options ImportOptions) importSectionResult {
	result := importSectionResult{
		warnings: []string{},
		errors:   []string{},
	}

	boardService := GetBoardService()
	if boardService == nil {
		result.errors = append(result.errors, "Board service not available")
		return result
	}

	existingBoards, _ := boardService.GetBoards(ctx)
	existingMap := make(map[string]*models.Board)
	for idx := range existingBoards {
		existingMap[existingBoards[idx].Name] = &existingBoards[idx]
	}

	for _, board := range boards {
		existing := existingMap[board.Name]

		if existing != nil {
			if options.MergeMode {
				// Skip existing in merge mode
				result.skipped++
				continue
			}

			if options.OverwriteBoards {
				// Update existing board
				board.Id = existing.Id
				board.CreatedAt = existing.CreatedAt
				board.UpdatedAt = time.Now()
				if err := boardService.UpdateBoard(ctx, board); err != nil {
					result.errors = append(result.errors, fmt.Sprintf("Failed to update board '%s': %v", board.Name, err))
				} else {
					result.updated++
				}
			} else {
				result.skipped++
			}
		} else {
			// Add new board
			board.CreatedAt = time.Now()
			board.UpdatedAt = time.Now()
			if err := boardService.AddBoard(ctx, board); err != nil {
				result.errors = append(result.errors, fmt.Sprintf("Failed to add board '%s': %v", board.Name, err))
			} else {
				result.added++
			}
		}
	}

	return result
}

// importRemotes imports remotes
func (i *ImportService) importRemotes(ctx context.Context, remotes []RemoteExport, options ImportOptions) importSectionResult {
	result := importSectionResult{
		warnings: []string{},
		errors:   []string{},
	}

	existingRemotes := fsConfig.GetRemotes()
	existingMap := make(map[string]bool)
	for _, r := range existingRemotes {
		existingMap[r.Name] = true
	}

	for _, remote := range remotes {
		exists := existingMap[remote.Name]

		if exists {
			if options.MergeMode {
				// Skip existing in merge mode
				result.skipped++
				continue
			}

			if options.OverwriteRemotes {
				// Delete and recreate
				fsConfig.DeleteRemote(remote.Name)

				rcParams := rc.Params{}
				for k, v := range remote.Config {
					rcParams[k] = v
				}

				// Use NonInteractive to prevent auth prompts
				opts := fsConfig.UpdateRemoteOpt{
					NonInteractive: true,
					NoObscure:      true, // Config values are already in final form
				}
				if _, err := fsConfig.CreateRemote(ctx, remote.Name, remote.Type, rcParams, opts); err != nil {
					result.errors = append(result.errors, fmt.Sprintf("Failed to update remote '%s': %v", remote.Name, err))
				} else {
					result.updated++
					// Check if token is missing
					if remote.Config["token"] == "" {
						result.warnings = append(result.warnings, fmt.Sprintf("Remote '%s' needs re-authentication", remote.Name))
					}
				}
			} else {
				result.skipped++
			}
		} else {
			// Add new remote
			rcParams := rc.Params{}
			for k, v := range remote.Config {
				rcParams[k] = v
			}

			// Use NonInteractive to prevent auth prompts
			opts := fsConfig.UpdateRemoteOpt{
				NonInteractive: true,
				NoObscure:      true, // Config values are already in final form
			}
			if _, err := fsConfig.CreateRemote(ctx, remote.Name, remote.Type, rcParams, opts); err != nil {
				result.errors = append(result.errors, fmt.Sprintf("Failed to add remote '%s': %v", remote.Name, err))
			} else {
				result.added++
				// Check if token is missing
				if remote.Config["token"] == "" {
					result.warnings = append(result.warnings, fmt.Sprintf("Remote '%s' needs re-authentication", remote.Name))
				}
			}
		}
	}

	return result
}
