package services

// GN Drive note: Coordinates app self-update checks, downloads, and staged installation.

import (
	"archive/tar"
	"archive/zip"
	"bufio"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/wailsapp/wails/v3/pkg/application"
)

const (
	updateLatestURL         = "https://api.github.com/repos/gnasdev/gn-drive/releases/latest"
	updateUserAgent         = "gn-drive-updater"
	updateHTTPTimeout       = 30 * time.Second
	updateStatusIdle        = "idle"
	updateStatusChecking    = "checking"
	updateStatusAvailable   = "available"
	updateStatusDownloading = "downloading"
	updateStatusDownloaded  = "downloaded"
	updateStatusInstalling  = "installing"
	updateStatusError       = "error"
)

// UpdateInfo describes the latest release relative to the running app.
type UpdateInfo struct {
	CurrentVersion string    `json:"current_version"`
	LatestVersion  string    `json:"latest_version"`
	HasUpdate      bool      `json:"has_update"`
	Unsupported    bool      `json:"unsupported"`
	Reason         string    `json:"reason,omitempty"`
	ReleaseURL     string    `json:"release_url"`
	PublishedAt    time.Time `json:"published_at"`
	Notes          string    `json:"notes"`
	AssetName      string    `json:"asset_name"`
	AssetURL       string    `json:"asset_url"`
	AssetSize      int64     `json:"asset_size"`
	ChecksumName   string    `json:"checksum_name,omitempty"`
	ChecksumURL    string    `json:"checksum_url,omitempty"`
	Checksum       string    `json:"checksum,omitempty"`
	Platform       string    `json:"platform"`
	Arch           string    `json:"arch"`
	DownloadedPath string    `json:"downloaded_path,omitempty"`
}

// UpdateStatus describes current updater state.
type UpdateStatus struct {
	Phase           string    `json:"phase"`
	Message         string    `json:"message"`
	CurrentVersion  string    `json:"current_version"`
	LatestVersion   string    `json:"latest_version,omitempty"`
	AssetName       string    `json:"asset_name,omitempty"`
	DownloadedBytes int64     `json:"downloaded_bytes"`
	TotalBytes      int64     `json:"total_bytes"`
	DownloadedPath  string    `json:"downloaded_path,omitempty"`
	Error           string    `json:"error,omitempty"`
	UpdatedAt       time.Time `json:"updated_at"`
}

// DownloadStatus describes a completed download.
type DownloadStatus struct {
	UpdateStatus
	SHA256 string `json:"sha256"`
}

type githubRelease struct {
	TagName     string               `json:"tag_name"`
	HTMLURL     string               `json:"html_url"`
	Body        string               `json:"body"`
	PublishedAt time.Time            `json:"published_at"`
	Assets      []githubReleaseAsset `json:"assets"`
}

type githubReleaseAsset struct {
	Name               string `json:"name"`
	BrowserDownloadURL string `json:"browser_download_url"`
	Size               int64  `json:"size"`
}

// UpdateService handles release discovery, update download, and staged installation.
type UpdateService struct {
	app            *application.App
	currentVersion string
	client         *http.Client
	latest         *UpdateInfo
	status         UpdateStatus
	mutex          sync.RWMutex
}

// NewUpdateService creates a new update service.
func NewUpdateService(app *application.App, currentVersion string) *UpdateService {
	if strings.TrimSpace(currentVersion) == "" {
		currentVersion = "dev"
	}
	return &UpdateService{
		app:            app,
		currentVersion: currentVersion,
		client:         &http.Client{Timeout: updateHTTPTimeout},
		status: UpdateStatus{
			Phase:          updateStatusIdle,
			Message:        "Ready",
			CurrentVersion: currentVersion,
			UpdatedAt:      time.Now(),
		},
	}
}

func (u *UpdateService) setApp(app *application.App) {
	u.mutex.Lock()
	defer u.mutex.Unlock()
	u.app = app
}

func (u *UpdateService) ServiceName() string {
	return "UpdateService"
}

func (u *UpdateService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("UpdateService starting up...")
	return nil
}

func (u *UpdateService) ServiceShutdown(ctx context.Context) error {
	log.Printf("UpdateService shutting down...")
	return nil
}

// CheckForUpdates checks the latest GitHub release and returns platform-specific update metadata.
func (u *UpdateService) CheckForUpdates(ctx context.Context) (UpdateInfo, error) {
	u.setStatus(updateStatusChecking, "Checking for updates", "", 0, 0, "")

	release, err := u.fetchLatestRelease(ctx)
	if err != nil {
		u.setError(err)
		return UpdateInfo{}, err
	}

	info, err := u.releaseToUpdateInfo(release)
	if err != nil {
		u.setError(err)
		return UpdateInfo{}, err
	}

	phase := updateStatusIdle
	message := "You are up to date"
	if info.Unsupported {
		message = info.Reason
	} else if info.HasUpdate {
		phase = updateStatusAvailable
		message = "Update available"
	}

	u.mutex.Lock()
	u.latest = &info
	u.status.Phase = phase
	u.status.Message = message
	u.status.CurrentVersion = u.currentVersion
	u.status.LatestVersion = info.LatestVersion
	u.status.AssetName = info.AssetName
	u.status.TotalBytes = info.AssetSize
	u.status.DownloadedBytes = 0
	u.status.Error = ""
	u.status.UpdatedAt = time.Now()
	u.mutex.Unlock()
	u.emitStatus()

	return info, nil
}

// DownloadLatestUpdate downloads and verifies the selected latest update asset.
func (u *UpdateService) DownloadLatestUpdate(ctx context.Context) (DownloadStatus, error) {
	u.mutex.RLock()
	latest := u.latest
	u.mutex.RUnlock()

	if latest == nil {
		info, err := u.CheckForUpdates(ctx)
		if err != nil {
			return DownloadStatus{}, err
		}
		latest = &info
	}

	if latest.Unsupported {
		err := errors.New(latest.Reason)
		u.setError(err)
		return DownloadStatus{}, err
	}
	if !latest.HasUpdate {
		err := fmt.Errorf("no update is available")
		u.setError(err)
		return DownloadStatus{}, err
	}
	if latest.ChecksumURL == "" {
		err := fmt.Errorf("release asset %q has no checksum file", latest.AssetName)
		u.setError(err)
		return DownloadStatus{}, err
	}

	checksum, err := u.fetchChecksum(ctx, latest.ChecksumURL, latest.AssetName)
	if err != nil {
		u.setError(err)
		return DownloadStatus{}, err
	}
	latest.Checksum = checksum

	cacheDir, err := updateCacheDir()
	if err != nil {
		u.setError(err)
		return DownloadStatus{}, err
	}
	if err := os.MkdirAll(cacheDir, 0755); err != nil {
		err = fmt.Errorf("failed to create update cache: %w", err)
		u.setError(err)
		return DownloadStatus{}, err
	}

	finalPath := filepath.Join(cacheDir, latest.AssetName)
	partialPath := finalPath + ".partial"
	if err := os.Remove(partialPath); err != nil && !errors.Is(err, os.ErrNotExist) {
		u.setError(err)
		return DownloadStatus{}, err
	}

	u.setStatus(updateStatusDownloading, "Downloading update", latest.AssetName, 0, latest.AssetSize, "")
	if err := u.downloadFile(ctx, latest.AssetURL, partialPath, latest.AssetSize); err != nil {
		u.setError(err)
		return DownloadStatus{}, err
	}

	actualChecksum, err := fileSHA256(partialPath)
	if err != nil {
		u.setError(err)
		return DownloadStatus{}, err
	}
	if !strings.EqualFold(actualChecksum, checksum) {
		err := fmt.Errorf("checksum mismatch for %s", latest.AssetName)
		u.setError(err)
		return DownloadStatus{}, err
	}

	if err := validateArchive(partialPath, latest.AssetName); err != nil {
		u.setError(err)
		return DownloadStatus{}, err
	}

	if err := os.Rename(partialPath, finalPath); err != nil {
		u.setError(err)
		return DownloadStatus{}, err
	}

	u.mutex.Lock()
	latest.DownloadedPath = finalPath
	u.latest = latest
	u.status.Phase = updateStatusDownloaded
	u.status.Message = "Update downloaded"
	u.status.AssetName = latest.AssetName
	u.status.DownloadedBytes = latest.AssetSize
	u.status.TotalBytes = latest.AssetSize
	u.status.DownloadedPath = finalPath
	u.status.Error = ""
	u.status.UpdatedAt = time.Now()
	status := u.status
	u.mutex.Unlock()
	u.emitStatus()

	return DownloadStatus{UpdateStatus: status, SHA256: actualChecksum}, nil
}

// InstallDownloadedUpdate stages and installs the downloaded update, then quits the current app.
func (u *UpdateService) InstallDownloadedUpdate(ctx context.Context) error {
	u.mutex.RLock()
	latest := u.latest
	app := u.app
	u.mutex.RUnlock()

	if latest == nil || latest.DownloadedPath == "" {
		err := fmt.Errorf("no downloaded update is ready to install")
		u.setError(err)
		return err
	}

	u.setStatus(updateStatusInstalling, "Installing update", latest.AssetName, latest.AssetSize, latest.AssetSize, latest.DownloadedPath)

	if err := installDownloadedArchive(latest.DownloadedPath, latest.AssetName); err != nil {
		u.setError(err)
		return err
	}

	if app != nil {
		go func() {
			time.Sleep(250 * time.Millisecond)
			app.Quit()
		}()
	}
	return nil
}

// GetUpdateStatus returns the current updater status.
func (u *UpdateService) GetUpdateStatus(ctx context.Context) UpdateStatus {
	u.mutex.RLock()
	defer u.mutex.RUnlock()
	return u.status
}

func (u *UpdateService) fetchLatestRelease(ctx context.Context) (githubRelease, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, updateLatestURL, nil)
	if err != nil {
		return githubRelease{}, err
	}
	req.Header.Set("Accept", "application/vnd.github+json")
	req.Header.Set("User-Agent", updateUserAgent)

	resp, err := u.client.Do(req)
	if err != nil {
		return githubRelease{}, fmt.Errorf("failed to check GitHub release: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return githubRelease{}, fmt.Errorf("GitHub release check failed: %s", resp.Status)
	}

	var release githubRelease
	if err := json.NewDecoder(resp.Body).Decode(&release); err != nil {
		return githubRelease{}, fmt.Errorf("failed to decode GitHub release: %w", err)
	}
	if release.TagName == "" {
		return githubRelease{}, fmt.Errorf("latest release has no tag")
	}
	return release, nil
}

func (u *UpdateService) releaseToUpdateInfo(release githubRelease) (UpdateInfo, error) {
	info := UpdateInfo{
		CurrentVersion: u.currentVersion,
		LatestVersion:  normalizeVersion(release.TagName),
		ReleaseURL:     release.HTMLURL,
		PublishedAt:    release.PublishedAt,
		Notes:          release.Body,
		Platform:       runtime.GOOS,
		Arch:           runtime.GOARCH,
	}

	if u.currentVersion == "dev" || u.currentVersion == "" {
		info.Unsupported = true
		info.Reason = "Development builds cannot be self-updated"
		return info, nil
	}

	hasUpdate, err := isVersionNewer(release.TagName, u.currentVersion)
	if err != nil {
		return info, err
	}
	info.HasUpdate = hasUpdate
	if !hasUpdate {
		return info, nil
	}

	asset, checksumAsset, ok := selectUpdateAsset(release.Assets, runtime.GOOS, runtime.GOARCH)
	if !ok {
		info.Unsupported = true
		info.Reason = fmt.Sprintf("No update asset is available for %s/%s", runtime.GOOS, runtime.GOARCH)
		return info, nil
	}

	info.AssetName = asset.Name
	info.AssetURL = asset.BrowserDownloadURL
	info.AssetSize = asset.Size
	info.ChecksumName = checksumAsset.Name
	info.ChecksumURL = checksumAsset.BrowserDownloadURL
	return info, nil
}

func (u *UpdateService) fetchChecksum(ctx context.Context, url, assetName string) (string, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return "", err
	}
	req.Header.Set("User-Agent", updateUserAgent)

	resp, err := u.client.Do(req)
	if err != nil {
		return "", fmt.Errorf("failed to download checksum: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("checksum download failed: %s", resp.Status)
	}

	body, err := io.ReadAll(io.LimitReader(resp.Body, 4096))
	if err != nil {
		return "", err
	}
	checksum, err := parseSHA256(string(body), assetName)
	if err != nil {
		return "", err
	}
	return checksum, nil
}

func (u *UpdateService) downloadFile(ctx context.Context, url, path string, total int64) error {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return err
	}
	req.Header.Set("User-Agent", updateUserAgent)

	resp, err := u.client.Do(req)
	if err != nil {
		return fmt.Errorf("failed to download update: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("update download failed: %s", resp.Status)
	}
	if total <= 0 {
		total = resp.ContentLength
	}

	file, err := os.OpenFile(path, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0644)
	if err != nil {
		return err
	}
	defer file.Close()

	buf := make([]byte, 64*1024)
	var downloaded int64
	lastEmit := time.Now()
	for {
		n, readErr := resp.Body.Read(buf)
		if n > 0 {
			if _, err := file.Write(buf[:n]); err != nil {
				return err
			}
			downloaded += int64(n)
			if time.Since(lastEmit) > 250*time.Millisecond || downloaded == total {
				u.setStatus(updateStatusDownloading, "Downloading update", filepath.Base(path), downloaded, total, "")
				lastEmit = time.Now()
			}
		}
		if readErr == io.EOF {
			break
		}
		if readErr != nil {
			return readErr
		}
	}
	if total > 0 && downloaded != total {
		return fmt.Errorf("downloaded size mismatch: got %d bytes, expected %d", downloaded, total)
	}
	return file.Sync()
}

func (u *UpdateService) setStatus(phase, message, assetName string, downloaded, total int64, downloadedPath string) {
	u.mutex.Lock()
	u.status.Phase = phase
	u.status.Message = message
	u.status.CurrentVersion = u.currentVersion
	u.status.AssetName = assetName
	u.status.DownloadedBytes = downloaded
	u.status.TotalBytes = total
	u.status.DownloadedPath = downloadedPath
	u.status.Error = ""
	u.status.UpdatedAt = time.Now()
	u.mutex.Unlock()
	u.emitStatus()
}

func (u *UpdateService) setError(err error) {
	u.mutex.Lock()
	u.status.Phase = updateStatusError
	u.status.Message = "Update failed"
	u.status.Error = err.Error()
	u.status.UpdatedAt = time.Now()
	u.mutex.Unlock()
	u.emitStatus()
}

func (u *UpdateService) emitStatus() {
	eventBus := GetSharedEventBus()
	if eventBus == nil {
		return
	}
	status := u.GetUpdateStatus(context.Background())
	event := map[string]interface{}{
		"type":      "update:status",
		"timestamp": time.Now(),
		"data":      status,
	}
	if err := eventBus.Emit(event); err != nil {
		log.Printf("Failed to emit update status: %v", err)
	}
}

func updateCacheDir() (string, error) {
	cfg := GetSharedConfig()
	if cfg == nil {
		return "", fmt.Errorf("shared config not set")
	}
	return filepath.Join(cfg.ConfigDir, "updates"), nil
}

func normalizeVersion(version string) string {
	version = strings.TrimSpace(version)
	version = strings.TrimPrefix(version, "v")
	version = strings.TrimPrefix(version, "V")
	return version
}

func isVersionNewer(candidate, current string) (bool, error) {
	candidateParts, err := parseSemver(candidate)
	if err != nil {
		return false, fmt.Errorf("invalid latest version %q: %w", candidate, err)
	}
	currentParts, err := parseSemver(current)
	if err != nil {
		return false, fmt.Errorf("invalid current version %q: %w", current, err)
	}
	for i := 0; i < 3; i++ {
		if candidateParts[i] > currentParts[i] {
			return true, nil
		}
		if candidateParts[i] < currentParts[i] {
			return false, nil
		}
	}
	return false, nil
}

func parseSemver(version string) ([3]int, error) {
	var result [3]int
	version = normalizeVersion(version)
	version = strings.Split(version, "-")[0]
	version = strings.Split(version, "+")[0]
	parts := strings.Split(version, ".")
	if len(parts) != 3 {
		return result, fmt.Errorf("expected major.minor.patch")
	}
	for i, part := range parts {
		value, err := strconv.Atoi(part)
		if err != nil || value < 0 {
			return result, fmt.Errorf("invalid numeric segment %q", part)
		}
		result[i] = value
	}
	return result, nil
}

func selectUpdateAsset(assets []githubReleaseAsset, goos, goarch string) (githubReleaseAsset, githubReleaseAsset, bool) {
	name := expectedAssetName(goos, goarch)
	if name == "" {
		return githubReleaseAsset{}, githubReleaseAsset{}, false
	}

	var asset githubReleaseAsset
	var checksum githubReleaseAsset
	for _, candidate := range assets {
		switch candidate.Name {
		case name:
			asset = candidate
		case name + ".sha256":
			checksum = candidate
		}
	}
	if asset.Name == "" || checksum.Name == "" {
		return githubReleaseAsset{}, githubReleaseAsset{}, false
	}
	return asset, checksum, true
}

func expectedAssetName(goos, goarch string) string {
	switch {
	case goos == "darwin" && goarch == "arm64":
		return "gn-drive-darwin-arm64.zip"
	case goos == "linux" && goarch == "amd64":
		return "gn-drive-linux-amd64.tar.gz"
	case goos == "windows" && goarch == "amd64":
		return "gn-drive-windows-amd64.zip"
	default:
		return ""
	}
}

func parseSHA256(content, assetName string) (string, error) {
	scanner := bufio.NewScanner(strings.NewReader(content))
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" {
			continue
		}
		fields := strings.Fields(line)
		if len(fields) == 1 && len(fields[0]) == 64 {
			return strings.ToLower(fields[0]), nil
		}
		if len(fields) >= 2 && strings.TrimPrefix(fields[1], "*") == assetName && len(fields[0]) == 64 {
			return strings.ToLower(fields[0]), nil
		}
	}
	if err := scanner.Err(); err != nil {
		return "", err
	}
	return "", fmt.Errorf("checksum file does not contain SHA256 for %s", assetName)
}

func fileSHA256(path string) (string, error) {
	file, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer file.Close()

	hash := sha256.New()
	if _, err := io.Copy(hash, file); err != nil {
		return "", err
	}
	return hex.EncodeToString(hash.Sum(nil)), nil
}

func validateArchive(path, assetName string) error {
	switch {
	case strings.HasSuffix(assetName, ".zip"):
		return validateZipArchive(path)
	case strings.HasSuffix(assetName, ".tar.gz"):
		return validateTarGzArchive(path)
	default:
		return fmt.Errorf("unsupported update archive: %s", assetName)
	}
}

func validateZipArchive(path string) error {
	reader, err := zip.OpenReader(path)
	if err != nil {
		return err
	}
	defer reader.Close()

	for _, file := range reader.File {
		if _, err := safeArchivePath("", file.Name); err != nil {
			return err
		}
	}
	return nil
}

func validateTarGzArchive(path string) error {
	file, err := os.Open(path)
	if err != nil {
		return err
	}
	defer file.Close()

	gz, err := gzip.NewReader(file)
	if err != nil {
		return err
	}
	defer gz.Close()

	tr := tar.NewReader(gz)
	for {
		header, err := tr.Next()
		if err == io.EOF {
			return nil
		}
		if err != nil {
			return err
		}
		if _, err := safeArchivePath("", header.Name); err != nil {
			return err
		}
	}
}

func safeArchivePath(root, name string) (string, error) {
	if name == "" {
		return "", fmt.Errorf("archive contains empty path")
	}
	cleanName := filepath.Clean(name)
	if filepath.IsAbs(cleanName) || cleanName == "." || strings.HasPrefix(cleanName, ".."+string(os.PathSeparator)) || cleanName == ".." {
		return "", fmt.Errorf("archive contains unsafe path %q", name)
	}
	if root == "" {
		return cleanName, nil
	}
	fullPath := filepath.Join(root, cleanName)
	cleanRoot := filepath.Clean(root)
	if fullPath != cleanRoot && !strings.HasPrefix(fullPath, cleanRoot+string(os.PathSeparator)) {
		return "", fmt.Errorf("archive path escapes target directory: %q", name)
	}
	return fullPath, nil
}

func installDownloadedArchive(path, assetName string) error {
	execPath, err := os.Executable()
	if err != nil {
		return fmt.Errorf("failed to locate current executable: %w", err)
	}

	stageDir, err := os.MkdirTemp("", "gn-drive-update-*")
	if err != nil {
		return err
	}

	if err := extractArchive(path, assetName, stageDir); err != nil {
		return err
	}

	switch runtime.GOOS {
	case "darwin":
		return installDarwinUpdate(stageDir, execPath)
	case "linux":
		return installLinuxUpdate(stageDir, execPath)
	case "windows":
		return installWindowsUpdate(stageDir, execPath)
	default:
		return fmt.Errorf("self update install is not supported on %s", runtime.GOOS)
	}
}

func extractArchive(path, assetName, targetDir string) error {
	switch {
	case strings.HasSuffix(assetName, ".zip"):
		return extractZip(path, targetDir)
	case strings.HasSuffix(assetName, ".tar.gz"):
		return extractTarGz(path, targetDir)
	default:
		return fmt.Errorf("unsupported update archive: %s", assetName)
	}
}

func extractZip(path, targetDir string) error {
	reader, err := zip.OpenReader(path)
	if err != nil {
		return err
	}
	defer reader.Close()

	for _, file := range reader.File {
		target, err := safeArchivePath(targetDir, file.Name)
		if err != nil {
			return err
		}
		if file.FileInfo().IsDir() {
			if err := os.MkdirAll(target, file.Mode()); err != nil {
				return err
			}
			continue
		}
		if err := os.MkdirAll(filepath.Dir(target), 0755); err != nil {
			return err
		}
		src, err := file.Open()
		if err != nil {
			return err
		}
		dst, err := os.OpenFile(target, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, file.Mode())
		if err != nil {
			src.Close()
			return err
		}
		_, copyErr := io.Copy(dst, src)
		closeErr := errors.Join(src.Close(), dst.Close())
		if copyErr != nil {
			return copyErr
		}
		if closeErr != nil {
			return closeErr
		}
	}
	return nil
}

func extractTarGz(path, targetDir string) error {
	file, err := os.Open(path)
	if err != nil {
		return err
	}
	defer file.Close()

	gz, err := gzip.NewReader(file)
	if err != nil {
		return err
	}
	defer gz.Close()

	tr := tar.NewReader(gz)
	for {
		header, err := tr.Next()
		if err == io.EOF {
			return nil
		}
		if err != nil {
			return err
		}
		target, err := safeArchivePath(targetDir, header.Name)
		if err != nil {
			return err
		}
		switch header.Typeflag {
		case tar.TypeDir:
			if err := os.MkdirAll(target, os.FileMode(header.Mode)); err != nil {
				return err
			}
		case tar.TypeReg:
			if err := os.MkdirAll(filepath.Dir(target), 0755); err != nil {
				return err
			}
			dst, err := os.OpenFile(target, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, os.FileMode(header.Mode))
			if err != nil {
				return err
			}
			_, copyErr := io.Copy(dst, tr)
			closeErr := dst.Close()
			if copyErr != nil {
				return copyErr
			}
			if closeErr != nil {
				return closeErr
			}
		default:
			return fmt.Errorf("archive contains unsupported entry type %q for %s", header.Typeflag, header.Name)
		}
	}
}

func installDarwinUpdate(stageDir, execPath string) error {
	bundleRoot, err := findAppBundleRoot(execPath)
	if err != nil {
		return err
	}
	stagedApp := filepath.Join(stageDir, "gn-drive.app")
	if stat, err := os.Stat(stagedApp); err != nil || !stat.IsDir() {
		return fmt.Errorf("downloaded update does not contain gn-drive.app")
	}

	scriptPath := filepath.Join(os.TempDir(), fmt.Sprintf("gn-drive-install-%d.sh", os.Getpid()))
	script := fmt.Sprintf(`#!/bin/sh
set -eu
while kill -0 %d 2>/dev/null; do
  sleep 0.2
done
rm -rf %s
cp -R %s %s
open %s
rm -f "$0"
`, os.Getpid(), shellQuote(bundleRoot), shellQuote(stagedApp), shellQuote(bundleRoot), shellQuote(bundleRoot))
	if err := os.WriteFile(scriptPath, []byte(script), 0700); err != nil {
		return err
	}
	return exec.Command("/bin/sh", scriptPath).Start()
}

func installLinuxUpdate(stageDir, execPath string) error {
	stagedBinary := filepath.Join(stageDir, "gn-drive")
	if stat, err := os.Stat(stagedBinary); err != nil || stat.IsDir() {
		return fmt.Errorf("downloaded update does not contain gn-drive binary")
	}
	if err := os.Chmod(stagedBinary, 0755); err != nil {
		return err
	}

	scriptPath := filepath.Join(os.TempDir(), fmt.Sprintf("gn-drive-install-%d.sh", os.Getpid()))
	script := fmt.Sprintf(`#!/bin/sh
set -eu
while kill -0 %d 2>/dev/null; do
  sleep 0.2
done
cp %s %s
chmod +x %s
%s >/dev/null 2>&1 &
rm -f "$0"
`, os.Getpid(), shellQuote(stagedBinary), shellQuote(execPath), shellQuote(execPath), shellQuote(execPath))
	if err := os.WriteFile(scriptPath, []byte(script), 0700); err != nil {
		return err
	}
	return exec.Command("/bin/sh", scriptPath).Start()
}

func installWindowsUpdate(stageDir, execPath string) error {
	stagedBinary := filepath.Join(stageDir, "gn-drive.exe")
	if stat, err := os.Stat(stagedBinary); err != nil || stat.IsDir() {
		return fmt.Errorf("downloaded update does not contain gn-drive.exe")
	}

	scriptPath := filepath.Join(os.TempDir(), fmt.Sprintf("gn-drive-install-%d.cmd", os.Getpid()))
	script := fmt.Sprintf(`@echo off
:wait
tasklist /FI "PID eq %d" 2>NUL | find "%d" >NUL
if not errorlevel 1 (
  timeout /t 1 /nobreak > NUL
  goto wait
)
copy /Y "%s" "%s"
start "" "%s"
del "%%~f0"
`, os.Getpid(), os.Getpid(), stagedBinary, execPath, execPath)
	if err := os.WriteFile(scriptPath, []byte(script), 0700); err != nil {
		return err
	}
	return exec.Command("cmd", "/C", "start", "", scriptPath).Start()
}

func findAppBundleRoot(execPath string) (string, error) {
	cleanPath := filepath.Clean(execPath)
	marker := string(os.PathSeparator) + "Contents" + string(os.PathSeparator) + "MacOS" + string(os.PathSeparator)
	idx := strings.LastIndex(cleanPath, marker)
	if idx < 0 {
		return "", fmt.Errorf("self update install requires running from a .app bundle")
	}
	bundleRoot := cleanPath[:idx]
	if !strings.HasSuffix(bundleRoot, ".app") {
		return "", fmt.Errorf("could not identify current app bundle")
	}
	return bundleRoot, nil
}

func shellQuote(value string) string {
	return "'" + strings.ReplaceAll(value, "'", "'\\''") + "'"
}
