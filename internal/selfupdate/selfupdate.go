// Package selfupdate downloads, verifies, and installs the latest
// gn-drive binary from GitHub Releases.
//
// Flow (per `docs/specs/planning/refactor-gn-drive-web-stack.md` §10):
//
//  1. GET https://api.github.com/repos/<owner>/<repo>/releases/latest
//  2. Compare tag_name (without leading "v") to current `version`.
//  3. Find the asset matching the current GOOS/GOARCH + archive format
//     (`.tar.gz` on POSIX, `.zip` on Windows) and its `.sha256` sidecar.
//  4. Download to a staging dir, verify SHA256.
//  5. Extract the binary, atomically rename the running binary to
//     `<bin>.bak`, and move the new binary into place.
//  6. Print restart instructions.
//
// All operations are non-destructive up to the atomic swap. If any step
// fails, the running binary is untouched and the staging dir is cleaned
// up.
package selfupdate

import (
	"archive/tar"
	"archive/zip"
	"bytes"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"time"
)

// Default values — overridable via Options for tests.
var (
	DefaultRepoOwner = "ngasdev"
	DefaultRepoName  = "gn-drive"

	// APIBase and DownloadBase are package-level variables so tests can
	// point them at a stub server. Production code should not modify them.
	APIBase      = "https://api.github.com"
	DownloadBase = "https://github.com"
	HTTPTimeout  = 60 * time.Second
)

// Options configures an Update call.
type Options struct {
	// RepoOwner and RepoName identify the GitHub repository.
	RepoOwner string
	RepoName  string

	// CurrentVersion is the version of the running binary, e.g. "0.4.0".
	// Empty means "force update regardless of version".
	CurrentVersion string

	// Force skips the version comparison and installs whatever is latest.
	Force bool

	// HTTPClient is the HTTP client to use. nil → http.DefaultClient with
	// HTTPTimeout. Tests can supply a custom client with a stub transport.
	HTTPClient *http.Client

	// StagingDir is where downloads are placed. Empty → os.TempDir().
	StagingDir string

	// Stdout is the writer for progress messages. nil → os.Stdout.
	Stdout io.Writer

	// Getenv is the environment lookup. nil → os.Getenv.
	Getenv func(string) string

	// Now returns the current time. nil → time.Now. Used by tests.
	Now func() time.Time
}

// Release mirrors the JSON shape of GitHub's release payload for the
// fields we care about. Unknown fields are ignored.
type Release struct {
	TagName string  `json:"tag_name"`
	Name    string  `json:"name"`
	Assets  []Asset `json:"assets"`
}

// Asset mirrors a release asset.
type Asset struct {
	Name               string `json:"name"`
	BrowserDownloadURL string `json:"browser_download_url"`
	Size               int64  `json:"size"`
}

// Result is returned to the caller.
type Result struct {
	Updated     bool      // false when already up-to-date (and !Force)
	OldVersion  string    // version of the running binary
	NewVersion  string    // version of the installed binary
	BinaryPath  string    // absolute path to the updated binary
	RestartHint string    // instruction printed to the user
	ReleasedAt  time.Time // when the release was published (best-effort)
}

// Errors returned by this package. Wrap with %w to inspect with errors.Is.
var (
	ErrNoRelease       = errors.New("selfupdate: no release found")
	ErrNoMatchingAsset = errors.New("selfupdate: no asset matches current OS/arch")
	ErrChecksumMismatch = errors.New("selfupdate: SHA256 mismatch")
	ErrAlreadyUpToDate = errors.New("selfupdate: already on latest version")
)

// Update runs the full pipeline and returns a Result. It does NOT replace
// the running binary on disk if the request is for "check only" — see
// Check.
func Update(ctx context.Context, opts Options) (*Result, error) {
	r, err := fetchReleaseFn(ctx, opts)
	if err != nil {
		return nil, err
	}
	newVersion := strings.TrimPrefix(r.TagName, "v")

	if !opts.Force && opts.CurrentVersion != "" && newVersion == opts.CurrentVersion {
		return &Result{
			Updated:     false,
			OldVersion:  opts.CurrentVersion,
			NewVersion:  newVersion,
			BinaryPath:  currentBinary(),
			RestartHint: "",
		}, ErrAlreadyUpToDate
	}

	asset, sumAsset, err := pickAssetsFn(r)
	if err != nil {
		return nil, err
	}

	stage, err := makeStagingDir(opts)
	if err != nil {
		return nil, fmt.Errorf("selfupdate: staging dir: %w", err)
	}
	defer func() {
		_ = os.RemoveAll(stage)
	}()

	archivePath := filepath.Join(stage, asset.Name)
	if err := downloadFn(ctx, opts, asset.BrowserDownloadURL, archivePath); err != nil {
		return nil, fmt.Errorf("selfupdate: download archive: %w", err)
	}

	expectedSum, err := fetchChecksumFn(ctx, opts, sumAsset, archivePath)
	if err != nil {
		return nil, fmt.Errorf("selfupdate: checksum: %w", err)
	}
	if err := verifyFileFn(archivePath, expectedSum); err != nil {
		return nil, err
	}

	binaryPath, err := extractBinaryFn(archivePath, stage)
	if err != nil {
		return nil, fmt.Errorf("selfupdate: extract: %w", err)
	}

	currentPath, err := osExecutableFn()
	if err != nil {
		return nil, fmt.Errorf("selfupdate: locate current binary: %w", err)
	}
	currentPath, err = evalSymlinksFn(currentPath)
	if err != nil {
		return nil, fmt.Errorf("selfupdate: resolve symlinks: %w", err)
	}

	if err := osChmodFn(binaryPath, 0o755); err != nil {
		return nil, fmt.Errorf("selfupdate: chmod: %w", err)
	}
	if err := atomicSwapFn(binaryPath, currentPath); err != nil {
		return nil, fmt.Errorf("selfupdate: swap: %w", err)
	}

	return &Result{
		Updated:     true,
		OldVersion:  opts.CurrentVersion,
		NewVersion:  newVersion,
		BinaryPath:  currentPath,
		RestartHint: "Restart gn-drive to use the new binary. Foreground: Ctrl+C and re-run. Service: 'gn-drive service restart'.",
	}, nil
}

// Check is a read-only variant that only reports whether an update is
// available. It performs no download.
func Check(ctx context.Context, opts Options) (current, latest string, err error) {
	r, err := fetchRelease(ctx, opts)
	if err != nil {
		return "", "", err
	}
	latest = strings.TrimPrefix(r.TagName, "v")
	current = opts.CurrentVersion
	return current, latest, nil
}

// --- internals ----------------------------------------------------------

func ownerName(opts Options) (string, string) {
	owner := opts.RepoOwner
	if owner == "" {
		owner = DefaultRepoOwner
	}
	name := opts.RepoName
	if name == "" {
		name = DefaultRepoName
	}
	return owner, name
}

func httpClient(opts Options) *http.Client {
	if opts.HTTPClient != nil {
		return opts.HTTPClient
	}
	return &http.Client{Timeout: HTTPTimeout}
}

func stdoutOf(opts Options) io.Writer {
	if opts.Stdout != nil {
		return opts.Stdout
	}
	return os.Stdout
}

func envOf(opts Options) func(string) string {
	if opts.Getenv != nil {
		return opts.Getenv
	}
	return os.Getenv
}

func stagingOf(opts Options) string {
	if opts.StagingDir != "" {
		return opts.StagingDir
	}
	return os.TempDir()
}

func fetchRelease(ctx context.Context, opts Options) (*Release, error) {
	owner, name := ownerName(opts)
	url := fmt.Sprintf("%s/repos/%s/%s/releases/latest", APIBase, owner, name)

	req, err := newHTTPRequest(ctx, http.MethodGet, url, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Accept", "application/vnd.github+json")
	if token := envOf(opts)("GITHUB_TOKEN"); token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}

	resp, err := httpClient(opts).Do(req)
	if err != nil {
		return nil, fmt.Errorf("selfupdate: GET %s: %w", url, err)
	}
	defer resp.Body.Close()
	if resp.StatusCode == http.StatusNotFound {
		return nil, ErrNoRelease
	}
	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(io.LimitReader(resp.Body, 1024))
		return nil, fmt.Errorf("selfupdate: GET %s: status %d: %s", url, resp.StatusCode, bytes.TrimSpace(body))
	}

	var rel Release
	if err := json.NewDecoder(resp.Body).Decode(&rel); err != nil {
		return nil, fmt.Errorf("selfupdate: decode release: %w", err)
	}
	if rel.TagName == "" {
		return nil, ErrNoRelease
	}
	return &rel, nil
}

// pickAssets selects the binary archive and its .sha256 sidecar for the
// current platform. Naming follows the release workflow:
//   gn-drive-<os>-<arch>.tar.gz / .zip
//   gn-drive-<os>-<arch>.tar.gz.sha256 / .zip.sha256
func pickAssets(r *Release) (*Asset, *Asset, error) {
	osName := runtime.GOOS
	archName := runtime.GOARCH
	ext := "tar.gz"
	if isWindows() {
		ext = "zip"
	}
	base := fmt.Sprintf("gn-drive-%s-%s.%s", osName, archName, ext)

	var bin *Asset
	for i := range r.Assets {
		if r.Assets[i].Name == base {
			bin = &r.Assets[i]
			break
		}
	}
	if bin == nil {
		return nil, nil, fmt.Errorf("%w: %s", ErrNoMatchingAsset, base)
	}
	sumName := base + ".sha256"
	var sum *Asset
	for i := range r.Assets {
		if r.Assets[i].Name == sumName {
			sum = &r.Assets[i]
			break
		}
	}
	if sum == nil {
		// Fall back to embedded checksum inside the archive if the sidecar
		// asset is missing. We don't support that here — return an error so
		// the operator sees the issue clearly.
		return nil, nil, fmt.Errorf("selfupdate: missing checksum asset %s", sumName)
	}
	return bin, sum, nil
}

// mkDirTemp is overridable for tests; defaults to os.MkdirTemp.
var mkDirTemp = os.MkdirTemp

func makeStagingDir(opts Options) (string, error) {
	dir, err := mkDirTemp(stagingOf(opts), "gn-drive-update-*")
	if err != nil {
		return "", err
	}
	return dir, nil
}

func download(ctx context.Context, opts Options, url, dest string) error {
	req, err := newHTTPRequest(ctx, http.MethodGet, url, nil)
	if err != nil {
		return err
	}
	resp, err := httpClient(opts).Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("download %s: status %d", url, resp.StatusCode)
	}

	f, err := os.OpenFile(dest, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o644)
	if err != nil {
		return err
	}
	defer f.Close()

	_, err = io.Copy(f, resp.Body)
	return err
}

// fetchChecksumFn is overridable for tests; defaults to fetchChecksum.
var fetchChecksumFn = fetchChecksum

// verifyFileFn is overridable for tests; defaults to verifyFile.
var verifyFileFn = verifyFile

// extractBinaryFn is overridable for tests; defaults to extractBinary.
var extractBinaryFn = extractBinary

// downloadFn is overridable for tests; defaults to download.
var downloadFn = download

// pickAssetsFn is overridable for tests; defaults to pickAssets.
var pickAssetsFn = pickAssets

// atomicSwapFn is overridable for tests; defaults to atomicSwap.
var atomicSwapFn = atomicSwap

// fetchReleaseFn is overridable for tests; defaults to fetchRelease.
var fetchReleaseFn = fetchRelease

// newHTTPRequest is overridable for tests; defaults to http.NewRequestWithContext.
var newHTTPRequest = http.NewRequestWithContext

// osExecutableFn is overridable for tests; defaults to os.Executable.
var osExecutableFn = os.Executable

// evalSymlinksFn is overridable for tests; defaults to filepath.EvalSymlinks.
var evalSymlinksFn = filepath.EvalSymlinks

// osChmodFn is overridable for tests; defaults to os.Chmod.
var osChmodFn = os.Chmod

// osRenameFn is overridable for tests; defaults to os.Rename.
var osRenameFn = os.Rename

// fetchChecksum downloads the .sha256 sidecar (preferred) or reads the
// local archive. The sidecar is a single line: "<hex>  <filename>\n".
func fetchChecksum(ctx context.Context, opts Options, sumAsset *Asset, archivePath string) (string, error) {
	req, err := newHTTPRequest(ctx, http.MethodGet, sumAsset.BrowserDownloadURL, nil)
	if err != nil {
		return "", err
	}
	resp, err := httpClient(opts).Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("download %s: status %d", sumAsset.BrowserDownloadURL, resp.StatusCode)
	}
	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", err
	}
	line := strings.TrimSpace(string(body))
	fields := strings.Fields(line)
	if len(fields) == 0 {
		return "", fmt.Errorf("selfupdate: empty checksum file")
	}
	sum := strings.ToLower(fields[0])
	if len(sum) != 64 {
		return "", fmt.Errorf("selfupdate: invalid checksum length %d", len(sum))
	}
	// Verify the sidecar claims to be about the archive we downloaded.
	if len(fields) >= 2 && filepath.Base(archivePath) != filepath.Base(fields[1]) {
		// Don't fail hard — some tools omit the filename. But log it.
		fmt.Fprintf(stdoutOf(opts), "warning: checksum sidecar filename %q does not match archive %q\n",
			fields[1], filepath.Base(archivePath))
	}
	return sum, nil
}

// fileOpenerFn is overridable for tests; defaults to os.Open.
var fileOpenerFn = func(path string) (io.ReadCloser, error) {
	return os.Open(path)
}

func verifyFile(path, expectedHex string) error {
	f, err := fileOpenerFn(path)
	if err != nil {
		return err
	}
	defer f.Close()
	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return err
	}
	got := hex.EncodeToString(h.Sum(nil))
	if got != expectedHex {
		return fmt.Errorf("%w: got %s, want %s", ErrChecksumMismatch, got, expectedHex)
	}
	return nil
}

// extractBinary pulls the gn-drive binary out of the archive into a
// writable location.
func extractBinary(archivePath, destDir string) (string, error) {
	if strings.HasSuffix(archivePath, ".zip") {
		return extractZip(archivePath, destDir)
	}
	return extractTarGz(archivePath, destDir)
}

func extractTarGz(archivePath, destDir string) (string, error) {
	f, err := os.Open(archivePath)
	if err != nil {
		return "", err
	}
	defer f.Close()
	gz, err := gzip.NewReader(f)
	if err != nil {
		return "", err
	}
	defer gz.Close()
	tr := tar.NewReader(gz)

	binaryName := "gn-drive"
	if isWindows() {
		binaryName = "gn-drive.exe"
	}
	for {
		hdr, err := tr.Next()
		if err == io.EOF {
			break
		}
		if err != nil {
			return "", err
		}
		if hdr.Typeflag != tar.TypeReg {
			continue
		}
		if filepath.Base(hdr.Name) != binaryName {
			continue
		}
		dest := filepath.Join(destDir, binaryName)
		out, err := os.OpenFile(dest, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o755)
		if err != nil {
			return "", err
		}
		if _, err := io.Copy(out, tr); err != nil {
			out.Close()
			return "", err
		}
		out.Close()
		return dest, nil
	}
	return "", fmt.Errorf("selfupdate: %s not found in archive", binaryName)
}

// zipFileOpenerFn is overridable for tests; defaults to (*zip.File).Open.
var zipFileOpenerFn = func(f *zip.File) (io.ReadCloser, error) { return f.Open() }

func extractZip(archivePath, destDir string) (string, error) {
	r, err := zip.OpenReader(archivePath)
	if err != nil {
		return "", err
	}
	defer r.Close()

	binaryName := "gn-drive.exe"
	for _, f := range r.File {
		if filepath.Base(f.Name) != binaryName {
			continue
		}
		src, err := zipFileOpenerFn(f)
		if err != nil {
			return "", err
		}
		dest := filepath.Join(destDir, binaryName)
		out, err := os.OpenFile(dest, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o755)
		if err != nil {
			src.Close()
			return "", err
		}
		if _, err := io.Copy(out, src); err != nil {
			src.Close()
			out.Close()
			return "", err
		}
		src.Close()
		out.Close()
		return dest, nil
	}
	return "", fmt.Errorf("selfupdate: %s not found in archive", binaryName)
}

// isWindows is overridable for tests so we can exercise the Windows branch
// on non-Windows platforms.
var isWindows = func() bool { return runtime.GOOS == "windows" }

// atomicSwap renames the running binary to <bin>.bak and moves the new
// binary into place. On Windows the running binary cannot be renamed
// while it is executing, so we use a two-step move: rename the running
// binary to <bin>.old, then move the new binary into the original path.
func atomicSwap(newBin, currentBin string) error {
	dir := filepath.Dir(currentBin)
	base := filepath.Base(currentBin)

	// Remove any leftover .bak from a previous failed update.
	_ = os.Remove(filepath.Join(dir, base+".bak"))

	if isWindows() {
		old := filepath.Join(dir, base+".old")
		_ = os.Remove(old)
		if err := osRenameFn(currentBin, old); err != nil {
			return fmt.Errorf("rename current to .old: %w", err)
		}
		if err := osRenameFn(newBin, currentBin); err != nil {
			// Try to restore.
			_ = osRenameFn(old, currentBin)
			return fmt.Errorf("rename new into place: %w", err)
		}
		_ = os.Remove(old)
		return nil
	}

	if err := osRenameFn(currentBin, filepath.Join(dir, base+".bak")); err != nil {
		return fmt.Errorf("rename current to .bak: %w", err)
	}
	if err := osRenameFn(newBin, currentBin); err != nil {
		// Try to restore from .bak.
		_ = osRenameFn(filepath.Join(dir, base+".bak"), currentBin)
		return fmt.Errorf("rename new into place: %w", err)
	}
	return nil
}

func currentBinary() string {
	p, err := osExecutableFn()
	if err != nil {
		return ""
	}
	if resolved, err := evalSymlinksFn(p); err == nil {
		return resolved
	}
	return p
}
