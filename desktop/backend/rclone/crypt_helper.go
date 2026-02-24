package rclone

import (
	"context"
	"desktop/backend/models"
	"fmt"
	"log"
	"strings"

	_ "github.com/rclone/rclone/backend/crypt"
	"github.com/rclone/rclone/fs/config"
	"github.com/rclone/rclone/fs/config/obscure"
	"github.com/rclone/rclone/fs/rc"

	"github.com/google/uuid"
)

const tempCryptPrefix = "_tmp_crypt_"

// ApplyCryptWrapping creates temporary crypt remotes for source/dest if configured in the profile.
// Returns a cleanup function that must be deferred to remove the temp remotes.
// The profile's From/To fields are modified in-place to point to the crypt remotes.
func ApplyCryptWrapping(ctx context.Context, profile *models.Profile) (cleanup func(), err error) {
	var tempRemotes []string

	cleanup = func() {
		for _, name := range tempRemotes {
			deleteTempCryptRemote(name)
		}
	}

	if !profile.EncryptSource && !profile.EncryptDest {
		return cleanup, nil
	}

	if profile.EncryptPassword == "" {
		return cleanup, fmt.Errorf("encryption password is required when encryption is enabled")
	}

	filenameEncrypt := profile.EncryptFilename
	if filenameEncrypt == "" {
		filenameEncrypt = "standard"
	}

	if profile.EncryptSource {
		remoteName := tempCryptPrefix + uuid.New().String()[:8]
		if err := createTempCryptRemote(ctx, remoteName, profile.From, profile.EncryptPassword, profile.EncryptPassword2, filenameEncrypt, profile.EncryptDirectory); err != nil {
			cleanup()
			return nil, fmt.Errorf("failed to create source crypt remote: %w", err)
		}
		tempRemotes = append(tempRemotes, remoteName)
		// Extract subpath from original remote (e.g., "gdrive:folder/sub" -> "folder/sub")
		subpath := ""
		if idx := strings.Index(profile.From, ":"); idx >= 0 {
			subpath = profile.From[idx+1:]
		}
		_ = subpath // subpath is already included in the wrapped remote path
		profile.From = remoteName + ":"
	}

	if profile.EncryptDest {
		remoteName := tempCryptPrefix + uuid.New().String()[:8]
		if err := createTempCryptRemote(ctx, remoteName, profile.To, profile.EncryptPassword, profile.EncryptPassword2, filenameEncrypt, profile.EncryptDirectory); err != nil {
			cleanup()
			return nil, fmt.Errorf("failed to create dest crypt remote: %w", err)
		}
		tempRemotes = append(tempRemotes, remoteName)
		profile.To = remoteName + ":"
	}

	// Clear passwords from profile after creating remotes (they're now in rclone config)
	profile.EncryptPassword = ""
	profile.EncryptPassword2 = ""

	// Update cleanup with final list
	finalRemotes := make([]string, len(tempRemotes))
	copy(finalRemotes, tempRemotes)
	cleanup = func() {
		for _, name := range finalRemotes {
			deleteTempCryptRemote(name)
		}
	}

	return cleanup, nil
}

// createTempCryptRemote creates a temporary crypt remote in rclone config.
func createTempCryptRemote(ctx context.Context, name, wrappedPath, password, password2, filenameEncrypt string, dirEncrypt bool) error {
	obscuredPassword, err := obscure.Obscure(password)
	if err != nil {
		return fmt.Errorf("failed to obscure password: %w", err)
	}

	dirNameEncrypt := "true"
	if !dirEncrypt {
		dirNameEncrypt = "false"
	}

	params := rc.Params{
		"remote":                    wrappedPath,
		"password":                  obscuredPassword,
		"filename_encryption":       filenameEncrypt,
		"directory_name_encryption": dirNameEncrypt,
	}

	if password2 != "" {
		obscuredPassword2, err := obscure.Obscure(password2)
		if err != nil {
			return fmt.Errorf("failed to obscure password2: %w", err)
		}
		params["password2"] = obscuredPassword2
	}

	_, err = config.CreateRemote(ctx, name, "crypt", params, config.UpdateRemoteOpt{
		NonInteractive: true,
		Obscure:        false,
	})
	if err != nil {
		return fmt.Errorf("failed to create crypt remote: %w", err)
	}

	log.Printf("Created temp crypt remote '%s' wrapping '%s'", name, wrappedPath)
	return nil
}

// deleteTempCryptRemote removes a temporary crypt remote from rclone config.
func deleteTempCryptRemote(name string) {
	config.DeleteRemote(name)
	log.Printf("Deleted temp crypt remote '%s'", name)
}

// CleanupOrphanedTempCryptRemotes removes any leftover temp crypt remotes from rclone config.
// Should be called on startup to clean up after crashes.
func CleanupOrphanedTempCryptRemotes() {
	remotes := config.FileSections()
	for _, r := range remotes {
		if strings.HasPrefix(r, tempCryptPrefix) {
			config.DeleteRemote(r)
			log.Printf("Cleaned up orphaned temp crypt remote '%s'", r)
		}
	}
}
