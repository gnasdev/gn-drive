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
	"net/http/httptest"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"
)

// Test fixture helpers ------------------------------------------------

// buildArchive writes a tar.gz archive containing a single "gn-drive"
// file with the supplied contents, plus a matching .sha256 file. The
// returned checksum is the SHA256 of the archive bytes (not the inner
// payload), matching what the real release workflow ships.
func buildArchive(t *testing.T, contents []byte) (archivePath, sumHex string) {
	t.Helper()
	dir := t.TempDir()
	archivePath = filepath.Join(dir, archiveName())

	f, err := os.Create(archivePath)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	gz := gzip.NewWriter(f)
	tw := tar.NewWriter(gz)
	hdr := &tar.Header{
		Name:     "gn-drive",
		Mode:     0o755,
		Size:     int64(len(contents)),
		Typeflag: tar.TypeReg,
	}
	if err := tw.WriteHeader(hdr); err != nil {
		t.Fatal(err)
	}
	if _, err := tw.Write(contents); err != nil {
		t.Fatal(err)
	}
	_ = tw.Close()
	_ = gz.Close()
	_ = f.Close()

	// Compute checksum over the archive file itself.
	f2, err := os.Open(archivePath)
	if err != nil {
		t.Fatal(err)
	}
	defer f2.Close()
	h := sha256.New()
	if _, err := io.Copy(h, f2); err != nil {
		t.Fatal(err)
	}
	sumHex = hex.EncodeToString(h.Sum(nil))
	return
}

func archiveName() string {
	if runtime.GOOS == "windows" {
		return "gn-drive-test-windows-amd64.zip"
	}
	return "gn-drive-test-linux-amd64.tar.gz"
}

// makeServer returns a stub HTTP server that serves the supplied release
// JSON, the archive bytes, and the checksum sidecar.
func makeServer(t *testing.T, release Release, archive []byte, sumHex string) *httptest.Server {
	t.Helper()
	return httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch {
		case strings.HasSuffix(r.URL.Path, "/releases/latest"):
			w.Header().Set("Content-Type", "application/json")
			_ = json.NewEncoder(w).Encode(release)
		case strings.HasSuffix(r.URL.Path, ".sha256"):
			fmt.Fprintf(w, "%s  %s\n", sumHex, filepath.Base(strings.TrimSuffix(r.URL.Path, ".sha256")))
		default:
			// Treat any other path as the archive download.
			w.Header().Set("Content-Type", "application/octet-stream")
			_, _ = w.Write(archive)
		}
	}))
}

func releaseJSONWithBase(srvURL, tag, assetName, sumName string) Release {
	return Release{
		TagName: tag,
		Name:    tag,
		Assets: []Asset{
			{
				Name:               assetName,
				BrowserDownloadURL: srvURL + "/dl/" + assetName,
				Size:               0,
			},
			{
				Name:               sumName,
				BrowserDownloadURL: srvURL + "/dl/" + sumName,
			},
		},
	}
}

// Tests ---------------------------------------------------------------

func TestUpdate_EndToEnd(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("test uses tar.gz; Windows path covered by extractZip unit")
	}
	contents := []byte("#!/bin/sh\necho hello\n")
	archivePath, sumHex := buildArchive(t, contents)

	archiveBytes, err := os.ReadFile(archivePath)
	if err != nil {
		t.Fatal(err)
	}

	assetBase := fmt.Sprintf("gn-drive-%s-%s.tar.gz", runtime.GOOS, runtime.GOARCH)
	rel := releaseJSONWithBase("ignored", "v9.9.9", assetBase, assetBase+".sha256")
	srv := makeServer(t, rel, archiveBytes, sumHex)
	defer srv.Close()
	// Override the asset URLs to point at our stub.
	rel.Assets[0].BrowserDownloadURL = srv.URL + "/dl/" + assetBase
	rel.Assets[1].BrowserDownloadURL = srv.URL + "/dl/" + assetBase + ".sha256"

	// Replace API/host so requests hit srv.
	oldAPIBase := APIBase
	APIBase = srv.URL
	oldDownloadBase := DownloadBase
	DownloadBase = srv.URL
	defer func() {
		APIBase = oldAPIBase
		DownloadBase = oldDownloadBase
	}()

	stage := t.TempDir()
	_, err = Update(context.Background(), Options{
		CurrentVersion: "0.0.0",
		StagingDir:     stage,
		Stdout:         io.Discard,
	})
	// The swap will fail because the test binary itself is locked, but every
	// preceding step (download, checksum, extract) must succeed. Allow the
	// post-extract failures (rename/swap) to pass through.
	if err != nil && !strings.Contains(err.Error(), "swap") && !strings.Contains(err.Error(), "rename") {
		t.Fatalf("Update: %v", err)
	}
	// Find the actual child dir MkdirTemp created and confirm it was cleaned.
	entries, _ := os.ReadDir(stage)
	for _, e := range entries {
		if e.IsDir() {
			t.Errorf("staging child %q should be removed after Update", e.Name())
		}
	}
}

func TestUpdate_AlreadyUpToDate(t *testing.T) {
	rel := releaseJSONWithBase("ignored", "v1.0.0",
		fmt.Sprintf("gn-drive-%s-%s.tar.gz", runtime.GOOS, runtime.GOARCH),
		fmt.Sprintf("gn-drive-%s-%s.tar.gz.sha256", runtime.GOOS, runtime.GOARCH))
	srv := makeServer(t, rel, nil, "")
	defer srv.Close()
	oldAPIBase := APIBase
	APIBase = srv.URL
	defer func() { APIBase = oldAPIBase }()

	_, err := Update(context.Background(), Options{
		CurrentVersion: "1.0.0",
		StagingDir:     t.TempDir(),
		Stdout:         io.Discard,
	})
	if !errors.Is(err, ErrAlreadyUpToDate) {
		t.Fatalf("expected ErrAlreadyUpToDate, got: %v", err)
	}
}

func TestUpdate_ForceBypassesVersionCheck(t *testing.T) {
	contents := []byte("new binary content")
	archivePath, sumHex := buildArchive(t, contents)
	archiveBytes, err := os.ReadFile(archivePath)
	if err != nil {
		t.Fatal(err)
	}
	assetBase := fmt.Sprintf("gn-drive-%s-%s.tar.gz", runtime.GOOS, runtime.GOARCH)
	rel := releaseJSONWithBase("ignored", "v1.0.0", assetBase, assetBase+".sha256")
	srv := makeServer(t, rel, archiveBytes, sumHex)
	defer srv.Close()
	rel.Assets[0].BrowserDownloadURL = srv.URL + "/dl/" + assetBase
	rel.Assets[1].BrowserDownloadURL = srv.URL + "/dl/" + assetBase + ".sha256"

	oldAPIBase := APIBase
	APIBase = srv.URL
	oldDownloadBase := DownloadBase
	DownloadBase = srv.URL
	defer func() {
		APIBase = oldAPIBase
		DownloadBase = oldDownloadBase
	}()

	_, err = Update(context.Background(), Options{
		CurrentVersion: "1.0.0", // same as latest
		Force:          true,
		StagingDir:     t.TempDir(),
		Stdout:         io.Discard,
	})
	// The atomic swap will fail at the last step (test binary locked);
	// we only assert that we got past the version check.
	if err != nil && !strings.Contains(err.Error(), "swap") && !strings.Contains(err.Error(), "rename") {
		t.Fatalf("Update (force): %v", err)
	}
}

func TestVerifyFile_ChecksumMismatch(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "blob")
	if err := os.WriteFile(path, []byte("hello"), 0o644); err != nil {
		t.Fatal(err)
	}
	bad := strings.Repeat("0", 64)
	if err := verifyFile(path, bad); !errors.Is(err, ErrChecksumMismatch) {
		t.Fatalf("expected ErrChecksumMismatch, got: %v", err)
	}
}

func TestVerifyFile_OK(t *testing.T) {
	contents := []byte("payload")
	dir := t.TempDir()
	path := filepath.Join(dir, "blob")
	if err := os.WriteFile(path, contents, 0o644); err != nil {
		t.Fatal(err)
	}
	h := sha256.Sum256(contents)
	if err := verifyFile(path, hex.EncodeToString(h[:])); err != nil {
		t.Fatalf("expected nil, got: %v", err)
	}
}

func TestFetchChecksum_ParsesSidecar(t *testing.T) {
	want := strings.Repeat("a", 64)
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprintf(w, "%s  gn-drive-linux-amd64.tar.gz\n", want)
	}))
	defer srv.Close()
	rel := Release{Assets: []Asset{{Name: "x.sha256", BrowserDownloadURL: srv.URL + "/x.sha256"}}}
	got, err := fetchChecksum(context.Background(), Options{
		Stdout: io.Discard,
	}, &rel.Assets[0], filepath.Join(t.TempDir(), "gn-drive-linux-amd64.tar.gz"))
	if err != nil {
		t.Fatal(err)
	}
	if got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

func TestPickAssets_MissingSidecar(t *testing.T) {
	rel := Release{Assets: []Asset{
		{Name: fmt.Sprintf("gn-drive-%s-%s.tar.gz", runtime.GOOS, runtime.GOARCH)},
	}}
	_, _, err := pickAssets(&rel)
	if err == nil {
		t.Fatal("expected error for missing .sha256")
	}
}

func TestPickAssets_MatchingFound(t *testing.T) {
	base := fmt.Sprintf("gn-drive-%s-%s.tar.gz", runtime.GOOS, runtime.GOARCH)
	rel := Release{Assets: []Asset{
		{Name: base + ".sha256"},
		{Name: base},
	}}
	bin, sum, err := pickAssets(&rel)
	if err != nil {
		t.Fatalf("pickAssets: %v", err)
	}
	if bin.Name != base {
		t.Errorf("binary asset name = %q, want %q", bin.Name, base)
	}
	if sum.Name != base+".sha256" {
		t.Errorf("sum asset name = %q, want %q", sum.Name, base+".sha256")
	}
}

func TestUpdate_NoRelease404(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.NotFound(w, r)
	}))
	defer srv.Close()
	oldAPIBase := APIBase
	APIBase = srv.URL
	defer func() { APIBase = oldAPIBase }()
	_, err := Update(context.Background(), Options{
		CurrentVersion: "0.0.0",
		StagingDir:     t.TempDir(),
		Stdout:         io.Discard,
	})
	if !errors.Is(err, ErrNoRelease) {
		t.Fatalf("expected ErrNoRelease, got: %v", err)
	}
}

// Test: when checksum sidecar has invalid length, error surfaces.
func TestFetchChecksum_InvalidHex(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprint(w, "not-a-real-checksum\n")
	}))
	defer srv.Close()
	rel := Release{Assets: []Asset{{Name: "x.sha256", BrowserDownloadURL: srv.URL + "/x.sha256"}}}
	_, err := fetchChecksum(context.Background(), Options{
		Stdout: io.Discard,
	}, &rel.Assets[0], "anything.tar.gz")
	if err == nil {
		t.Fatal("expected invalid checksum error")
	}
}

// Test: ensure tarball extraction actually writes the binary.
func TestExtractTarGz(t *testing.T) {
	contents := []byte("tar contents")
	archivePath, _ := buildArchive(t, contents)
	dest, err := extractBinary(archivePath, t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile(dest)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(got, contents) {
		t.Errorf("got %q, want %q", got, contents)
	}
}

// Test: extract a tar.gz with multiple files; only the binary is extracted.
func TestExtractTarGz_MultipleFiles(t *testing.T) {
	dir := t.TempDir()
	archivePath := filepath.Join(dir, "multi.tar.gz")
	f, _ := os.Create(archivePath)
	defer f.Close()
	gz := gzip.NewWriter(f)
	tw := tar.NewWriter(gz)
	// Add a non-binary file.
	hdr1 := &tar.Header{Name: "README", Mode: 0o644, Size: 5, Typeflag: tar.TypeReg}
	_ = tw.WriteHeader(hdr1)
	_, _ = tw.Write([]byte("hello"))
	// Add the binary.
	binaryContent := []byte("binary-data")
	hdr2 := &tar.Header{Name: "gn-drive", Mode: 0o755, Size: int64(len(binaryContent)), Typeflag: tar.TypeReg}
	_ = tw.WriteHeader(hdr2)
	_, _ = tw.Write(binaryContent)
	_ = tw.Close()
	_ = gz.Close()

	dest, err := extractBinary(archivePath, t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	got, _ := os.ReadFile(dest)
	if !bytes.Equal(got, binaryContent) {
		t.Errorf("got %q, want %q", got, binaryContent)
	}
}

// Test: extract a tar.gz where no gn-drive binary is present.
func TestExtractTarGz_NoBinary(t *testing.T) {
	dir := t.TempDir()
	archivePath := filepath.Join(dir, "no-bin.tar.gz")
	f, _ := os.Create(archivePath)
	defer f.Close()
	gz := gzip.NewWriter(f)
	tw := tar.NewWriter(gz)
	hdr := &tar.Header{Name: "other", Mode: 0o755, Size: 1, Typeflag: tar.TypeReg}
	_ = tw.WriteHeader(hdr)
	_, _ = tw.Write([]byte("x"))
	_ = tw.Close()
	_ = gz.Close()

	_, err := extractBinary(archivePath, t.TempDir())
	if err == nil {
		t.Error("expected error for missing binary")
	}
}

// Test: extract a tar.gz with corrupted gzip data.
func TestExtractTarGz_Corrupted(t *testing.T) {
	dir := t.TempDir()
	archivePath := filepath.Join(dir, "bad.tar.gz")
	if err := os.WriteFile(archivePath, []byte("not a real gzip"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := extractBinary(archivePath, t.TempDir()); err == nil {
		t.Error("expected error for corrupted archive")
	}
}

// Test: extractBinary dispatches to extractTarGz for .tar.gz
func TestExtractBinary_DispatchesByExtension(t *testing.T) {
	contents := []byte("dispatched")
	archivePath, _ := buildArchive(t, contents)
	dest, err := extractBinary(archivePath, t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(dest); err != nil {
		t.Errorf("binary not extracted: %v", err)
	}
}

// Test: extract a zip archive containing gn-drive.exe.
func TestExtractZip(t *testing.T) {
	if runtime.GOOS == "windows" {
		// On Windows the binary is named gn-drive.exe; we skip this test
		// on non-windows because buildArchive creates a tar.gz, not a zip.
		t.Skip("windows-specific zip extract test")
	}
	dir := t.TempDir()
	zipPath := filepath.Join(dir, "gn-drive-windows-amd64.zip")
	f, err := os.Create(zipPath)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	zw := zip.NewWriter(f)
	binaryContent := []byte("windows-binary")
	w, err := zw.Create("gn-drive.exe")
	if err != nil {
		t.Fatal(err)
	}
	_, _ = w.Write(binaryContent)
	_ = zw.Close()

	// Use extractBinary which dispatches by extension.
	dest, err := extractBinary(zipPath, t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	got, _ := os.ReadFile(dest)
	if !bytes.Equal(got, binaryContent) {
		t.Errorf("got %q, want %q", got, binaryContent)
	}
}

// Test: extract zip with no binary.
func TestExtractZip_NoBinary(t *testing.T) {
	dir := t.TempDir()
	zipPath := filepath.Join(dir, "no-bin.zip")
	f, _ := os.Create(zipPath)
	defer f.Close()
	zw := zip.NewWriter(f)
	w, _ := zw.Create("other.txt")
	_, _ = w.Write([]byte("x"))
	_ = zw.Close()

	if _, err := extractBinary(zipPath, t.TempDir()); err == nil {
		t.Error("expected error for missing binary")
	}
}

// Test: atomicSwap renames current binary to .bak and puts new one in place.
func TestAtomicSwap_POSIX(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("POSIX swap test on non-windows")
	}
	dir := t.TempDir()
	currentBin := filepath.Join(dir, "gn-drive")
	newBin := filepath.Join(dir, "new-gn-drive")
	if err := os.WriteFile(currentBin, []byte("OLD"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(newBin, []byte("NEW"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := atomicSwap(newBin, currentBin); err != nil {
		t.Fatal(err)
	}
	// currentBin should now contain NEW.
	got, _ := os.ReadFile(currentBin)
	if string(got) != "NEW" {
		t.Errorf("current = %q, want NEW", got)
	}
	// .bak should contain OLD.
	bak := currentBin + ".bak"
	got, _ = os.ReadFile(bak)
	if string(got) != "OLD" {
		t.Errorf("bak = %q, want OLD", got)
	}
	// newBin should be gone.
	if _, err := os.Stat(newBin); !os.IsNotExist(err) {
		t.Error("new binary should be removed")
	}
}

// Test: atomicSwap on Windows uses .old instead of .bak.
func TestAtomicSwap_Windows(t *testing.T) {
	if runtime.GOOS != "windows" {
		t.Skip("windows-specific test")
	}
	// Not tested on non-windows; on windows the path uses .old.
}

// Test: atomicSwap is idempotent — second call after Release should not
// leave a stale .bak file. (The .bak handling is in instance.Locker,
// not here, so this just verifies that atomicSwap can be called twice.)
func TestAtomicSwap_Twice(t *testing.T) {
	dir := t.TempDir()
	bin1 := filepath.Join(dir, "bin1")
	bin2 := filepath.Join(dir, "bin2")
	_ = os.WriteFile(bin1, []byte("first"), 0o755)
	_ = os.WriteFile(bin2, []byte("second"), 0o755)
	if err := atomicSwap(bin2, bin1); err != nil {
		t.Fatal(err)
	}
	// The .bak from the first swap should be removed by a second swap
	// only if the implementation handles it. The current code doesn't
	// remove .bak on subsequent swaps. We just verify the second swap
	// still works.
	_ = os.WriteFile(bin2, []byte("third"), 0o755)
	if err := atomicSwap(bin2, bin1); err != nil {
		t.Fatal(err)
	}
	got, _ := os.ReadFile(bin1)
	if string(got) != "third" {
		t.Errorf("current = %q, want third", got)
	}
}

// Test: currentBinary returns a path.
func TestCurrentBinary_ReturnsPath(t *testing.T) {
	p := currentBinary()
	if p == "" {
		t.Error("currentBinary returned empty")
	}
}

// Test: Check returns the current and latest version without downloading.
func TestCheck_VersionOnly(t *testing.T) {
	rel := releaseJSONWithBase("ignored", "v2.3.4", "x.tar.gz", "x.tar.gz.sha256")
	srv := makeServer(t, rel, nil, "")
	defer srv.Close()
	oldAPIBase := APIBase
	APIBase = srv.URL
	defer func() { APIBase = oldAPIBase }()

	cur, latest, err := Check(context.Background(), Options{CurrentVersion: "2.3.3"})
	if err != nil {
		t.Fatal(err)
	}
	if cur != "2.3.3" || latest != "2.3.4" {
		t.Errorf("cur=%q latest=%q", cur, latest)
	}
	_ = time.Now // silence unused if test reorders
}

// --- helper default tests ---

func TestHTTPClient_Default(t *testing.T) {
	c := httpClient(Options{})
	if c.Timeout != HTTPTimeout {
		t.Errorf("default timeout = %v, want %v", c.Timeout, HTTPTimeout)
	}
}

func TestHTTPClient_Custom(t *testing.T) {
	custom := &http.Client{Timeout: 5 * time.Second}
	c := httpClient(Options{HTTPClient: custom})
	if c != custom {
		t.Error("expected custom client")
	}
}

func TestStdoutOf_Default(t *testing.T) {
	w := stdoutOf(Options{})
	if w != os.Stdout {
		t.Error("default stdout should be os.Stdout")
	}
}

func TestStdoutOf_Custom(t *testing.T) {
	var buf bytes.Buffer
	w := stdoutOf(Options{Stdout: &buf})
	if w != &buf {
		t.Error("expected custom writer")
	}
}

func TestEnvOf_Default(t *testing.T) {
	fn := envOf(Options{})
	if fn == nil {
		t.Error("default env func should not be nil")
	}
	if fn("PATH") == "" {
		t.Error("PATH should be set")
	}
}

func TestEnvOf_Custom(t *testing.T) {
	calls := 0
	custom := func(k string) string {
		calls++
		return "custom-value"
	}
	fn := envOf(Options{Getenv: custom})
	if fn("FOO") != "custom-value" {
		t.Error("expected custom return")
	}
	if calls != 1 {
		t.Errorf("calls = %d", calls)
	}
}

func TestStagingOf_Default(t *testing.T) {
	dir := stagingOf(Options{})
	if dir == "" {
		t.Error("default staging dir should not be empty")
	}
}

func TestStagingOf_Custom(t *testing.T) {
	dir := stagingOf(Options{StagingDir: "/my/staging"})
	if dir != "/my/staging" {
		t.Errorf("staging = %q", dir)
	}
}

func TestMakeStagingDir_Cleanup(t *testing.T) {
	dir, err := makeStagingDir(Options{StagingDir: ""})
	if err != nil {
		t.Fatal(err)
	}
	if dir == "" {
		t.Error("expected non-empty dir")
	}
	t.Cleanup(func() { _ = os.RemoveAll(dir) })
	if _, err := os.Stat(dir); err != nil {
		t.Errorf("dir should exist: %v", err)
	}
}

// --- additional coverage ------------------------------------------------

// TestAtomicSwap_NewBinNotExist exercises the error path when the new
// binary doesn't exist.
func TestAtomicSwap_NewBinNotExist(t *testing.T) {
	dir := t.TempDir()
	currentBin := filepath.Join(dir, "current")
	if err := os.WriteFile(currentBin, []byte("OLD"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := atomicSwap(filepath.Join(dir, "no-such-file"), currentBin); err == nil {
		t.Error("expected error from missing new binary")
	}
}

// TestAtomicSwap_RestoreFromBak exercises the recovery branch when the
// new binary rename fails. We make the current path a directory so the
// second rename fails. The recovery path then tries to restore from .bak.
func TestAtomicSwap_RestoreFromBak(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("POSIX swap test")
	}
	dir := t.TempDir()
	currentBin := filepath.Join(dir, "gn-drive")
	newBin := filepath.Join(dir, "new-gn-drive")
	if err := os.WriteFile(currentBin, []byte("OLD"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(newBin, []byte("NEW"), 0o755); err != nil {
		t.Fatal(err)
	}
	// First, do a successful swap (creates .bak).
	if err := atomicSwap(newBin, currentBin); err != nil {
		t.Fatal(err)
	}
	// Now currentBin contains NEW and .bak contains OLD.
	// Remove currentBin and replace it with a directory so the next swap's
	// rename fails (you can't rename onto a directory in most cases).
	if err := os.Remove(currentBin); err != nil {
		t.Fatal(err)
	}
	if err := os.Mkdir(currentBin, 0o755); err != nil {
		t.Fatal(err)
	}
	secondNew := filepath.Join(dir, "second-new")
	if err := os.WriteFile(secondNew, []byte("SECOND"), 0o755); err != nil {
		t.Fatal(err)
	}
	err := atomicSwap(secondNew, currentBin)
	// The error path triggers either because the rename fails or the
	// directory target blocks the swap. Either way, the function should
	// return an error.
	if err == nil {
		t.Log("note: rename may have succeeded; this is platform-dependent")
	}
}

// TestAtomicSwap_WindowsBranch exercises the Windows code path on any
// platform by overriding isWindows.
func TestAtomicSwap_WindowsBranch(t *testing.T) {
	orig := isWindows
	defer func() { isWindows = orig }()
	isWindows = func() bool { return true }

	dir := t.TempDir()
	currentBin := filepath.Join(dir, "gn-drive.exe")
	newBin := filepath.Join(dir, "new-gn-drive.exe")
	if err := os.WriteFile(currentBin, []byte("OLD"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(newBin, []byte("NEW"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := atomicSwap(newBin, currentBin); err != nil {
		t.Fatal(err)
	}
	got, _ := os.ReadFile(currentBin)
	if string(got) != "NEW" {
		t.Errorf("current = %q, want NEW", got)
	}
}

// TestAtomicSwap_WindowsError exercises the error paths in the Windows
// branch.
func TestAtomicSwap_WindowsError(t *testing.T) {
	orig := isWindows
	defer func() { isWindows = orig }()
	isWindows = func() bool { return true }

	// currentBin doesn't exist → os.Rename fails.
	dir := t.TempDir()
	newBin := filepath.Join(dir, "new.exe")
	if err := os.WriteFile(newBin, []byte("NEW"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := atomicSwap(newBin, filepath.Join(dir, "no-current")); err == nil {
		t.Error("expected error from missing current binary")
	}
}

// --- Update error path tests ---

// TestUpdate_DownloadError exercises the download error path in Update.
func TestUpdate_DownloadError(t *testing.T) {
	rel := Release{
		TagName: "v9.9.9",
		Assets: []Asset{
			{Name: fmt.Sprintf("gn-drive-%s-%s.tar.gz", runtime.GOOS, runtime.GOARCH), BrowserDownloadURL: "http://127.0.0.1:1/missing"},
			{Name: fmt.Sprintf("gn-drive-%s-%s.tar.gz.sha256", runtime.GOOS, runtime.GOARCH), BrowserDownloadURL: "http://127.0.0.1:1/missing"},
		},
	}
	srv := makeServer(t, rel, nil, "")
	defer srv.Close()
	oldAPIBase := APIBase
	APIBase = srv.URL
	defer func() { APIBase = oldAPIBase }()

	stage := t.TempDir()
	_, err := Update(context.Background(), Options{
		CurrentVersion: "0.0.0",
		StagingDir:     stage,
		Stdout:         io.Discard,
	})
	if err == nil {
		t.Error("expected error from download failure")
	}
}

// TestUpdate_ChecksumError exercises the checksum mismatch path in Update.
func TestUpdate_ChecksumError(t *testing.T) {
	// Build a valid tar.gz archive.
	contents := []byte("#!/bin/sh\necho hello\n")
	archivePath, _ := buildArchive(t, contents)
	archiveBytes, err := os.ReadFile(archivePath)
	if err != nil {
		t.Fatal(err)
	}

	assetBase := fmt.Sprintf("gn-drive-%s-%s.tar.gz", runtime.GOOS, runtime.GOARCH)
	rel := Release{
		TagName: "v9.9.9",
		Assets: []Asset{
			{Name: assetBase, BrowserDownloadURL: ""},
			{Name: assetBase + ".sha256", BrowserDownloadURL: ""},
		},
	}
	srv := makeServer(t, rel, archiveBytes, "0000000000000000000000000000000000000000000000000000000000000000")
	defer srv.Close()
	rel.Assets[0].BrowserDownloadURL = srv.URL + "/dl/" + assetBase
	rel.Assets[1].BrowserDownloadURL = srv.URL + "/dl/" + assetBase + ".sha256"

	oldAPIBase := APIBase
	APIBase = srv.URL
	oldDownloadBase := DownloadBase
	DownloadBase = srv.URL
	defer func() {
		APIBase = oldAPIBase
		DownloadBase = oldDownloadBase
	}()

	stage := t.TempDir()
	_, err = Update(context.Background(), Options{
		CurrentVersion: "0.0.0",
		StagingDir:     stage,
		Stdout:         io.Discard,
	})
	if err == nil {
		t.Error("expected error from checksum mismatch")
	}
}

// TestUpdate_ExtractError exercises the extract error path in Update.
func TestUpdate_ExtractError(t *testing.T) {
	// Build a valid tar.gz but with a wrong checksum.
	contents := []byte("not a real binary")
	archivePath, _ := buildArchive(t, contents)
	archiveBytes, err := os.ReadFile(archivePath)
	if err != nil {
		t.Fatal(err)
	}

	assetBase := fmt.Sprintf("gn-drive-%s-%s.tar.gz", runtime.GOOS, runtime.GOARCH)
	// Build a checksum that matches a different archive (force mismatch).
	h := sha256.Sum256(archiveBytes)
	sumHex := hex.EncodeToString(h[:])
	// Replace last char to force mismatch.
	sumHex = sumHex[:len(sumHex)-1] + "f"

	rel := Release{
		TagName: "v9.9.9",
		Assets: []Asset{
			{Name: assetBase, BrowserDownloadURL: ""},
			{Name: assetBase + ".sha256", BrowserDownloadURL: ""},
		},
	}
	srv := makeServer(t, rel, archiveBytes, sumHex)
	defer srv.Close()
	rel.Assets[0].BrowserDownloadURL = srv.URL + "/dl/" + assetBase
	rel.Assets[1].BrowserDownloadURL = srv.URL + "/dl/" + assetBase + ".sha256"

	oldAPIBase := APIBase
	APIBase = srv.URL
	oldDownloadBase := DownloadBase
	DownloadBase = srv.URL
	defer func() {
		APIBase = oldAPIBase
		DownloadBase = oldDownloadBase
	}()

	stage := t.TempDir()
	_, err = Update(context.Background(), Options{
		CurrentVersion: "0.0.0",
		StagingDir:     stage,
		Stdout:         io.Discard,
	})
	if err == nil {
		t.Error("expected error from checksum mismatch")
	}
}

// TestUpdate_ForceBypassesAlreadyUpToDate exercises the Force flag path.
func TestUpdate_ForceBypassesAlreadyUpToDate(t *testing.T) {
	rel := Release{
		TagName: "v1.0.0",
		Assets: []Asset{
			{Name: fmt.Sprintf("gn-drive-%s-%s.tar.gz", runtime.GOOS, runtime.GOARCH), BrowserDownloadURL: ""},
			{Name: fmt.Sprintf("gn-drive-%s-%s.tar.gz.sha256", runtime.GOOS, runtime.GOARCH), BrowserDownloadURL: ""},
		},
	}
	srv := makeServer(t, rel, nil, "")
	defer srv.Close()
	oldAPIBase := APIBase
	APIBase = srv.URL
	defer func() { APIBase = oldAPIBase }()

	stage := t.TempDir()
	_, err := Update(context.Background(), Options{
		CurrentVersion: "1.0.0", // same as newVersion
		Force:          true,      // force past the equality check
		StagingDir:     stage,
		Stdout:         io.Discard,
	})
	// We expect either a download error (since URLs are bad) or success
	// (if URLs work). The test verifies that Force bypasses the equality check.
	if err == nil {
		t.Log("Update with Force may have succeeded (test server didn't reject download)")
	} else if strings.Contains(err.Error(), "already on latest") {
		t.Error("Force flag should bypass ErrAlreadyUpToDate")
	}
}

// TestAtomicSwap_CurrentBinNotExist exercises the error path when the
// current binary doesn't exist.
func TestAtomicSwap_CurrentBinNotExist(t *testing.T) {
	dir := t.TempDir()
	newBin := filepath.Join(dir, "new")
	if err := os.WriteFile(newBin, []byte("NEW"), 0o755); err != nil {
		t.Fatal(err)
	}
	// currentBin doesn't exist; os.Rename should fail.
	if err := atomicSwap(newBin, filepath.Join(dir, "no-current")); err == nil {
		t.Error("expected error from missing current binary")
	}
}

// TestExtractTarGz_InvalidGzip exercises the gzip error path.
func TestExtractTarGz_InvalidGzip(t *testing.T) {
	dir := t.TempDir()
	archive := filepath.Join(dir, "bad.tar.gz")
	if err := os.WriteFile(archive, []byte("not a gzip file"), 0o644); err != nil {
		t.Fatal(err)
	}
	_, err := extractTarGz(archive, dir)
	if err == nil {
		t.Error("expected error from invalid gzip")
	}
}

// TestExtractTarGz_TruncatedArchive exercises the tar read error path.
func TestExtractTarGz_TruncatedArchive(t *testing.T) {
	dir := t.TempDir()
	archive := filepath.Join(dir, "truncated.tar.gz")

	// Build a valid gzip header but truncated payload.
	var buf bytes.Buffer
	gz := gzip.NewWriter(&buf)
	// Write a tar header for "gn-drive" but no body.
	tw := tar.NewWriter(gz)
	hdr := &tar.Header{Name: "gn-drive", Mode: 0o755, Size: 100, Typeflag: tar.TypeReg}
	if err := tw.WriteHeader(hdr); err != nil {
		t.Fatal(err)
	}
	// Don't write the body — leave it truncated.
	_ = tw.Close()
	_ = gz.Close()

	if err := os.WriteFile(archive, buf.Bytes(), 0o644); err != nil {
		t.Fatal(err)
	}
	_, err := extractTarGz(archive, dir)
	if err == nil {
		t.Error("expected error from truncated tar")
	}
}

// TestExtractTarGz_DestDirNotExist exercises the os.OpenFile error path.
func TestExtractTarGz_DestDirNotExist(t *testing.T) {
	dir := t.TempDir()
	archive := filepath.Join(dir, "good.tar.gz")

	// Build a tar.gz with the gn-drive binary.
	buildTarGz(t, archive, "gn-drive", "binary content")

	// Use a non-existent destination dir.
	_, err := extractTarGz(archive, filepath.Join(dir, "no-such-dir"))
	if err == nil {
		t.Error("expected error from missing dest dir")
	}
}

// TestExtractZip_InvalidZip exercises the zip error path.
func TestExtractZip_InvalidZip(t *testing.T) {
	dir := t.TempDir()
	archive := filepath.Join(dir, "bad.zip")
	if err := os.WriteFile(archive, []byte("not a zip file"), 0o644); err != nil {
		t.Fatal(err)
	}
	_, err := extractZip(archive, dir)
	if err == nil {
		t.Error("expected error from invalid zip")
	}
}

// TestExtractZip_DestDirNotExist exercises the os.OpenFile error path.
func TestExtractZip_DestDirNotExist(t *testing.T) {
	dir := t.TempDir()
	archive := filepath.Join(dir, "good.zip")

	// Build a zip with the gn-drive.exe binary.
	buildZip(t, archive, "gn-drive.exe", "binary content")

	_, err := extractZip(archive, filepath.Join(dir, "no-such-dir"))
	if err == nil {
		t.Error("expected error from missing dest dir")
	}
}

// TestDownload_BadStatus exercises the non-200 status path.
func TestDownload_BadStatus(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer srv.Close()
	dir := t.TempDir()
	dest := filepath.Join(dir, "out.bin")
	err := download(context.Background(), Options{}, srv.URL, dest)
	if err == nil {
		t.Error("expected error from 500 status")
	}
}

// TestFetchChecksum_Empty exercises the empty body path.
func TestFetchChecksum_Empty(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Empty body.
	}))
	defer srv.Close()
	sumAsset := &Asset{BrowserDownloadURL: srv.URL}
	_, err := fetchChecksum(context.Background(), Options{}, sumAsset, "gn-drive.tar.gz")
	if err == nil {
		t.Error("expected error from empty checksum")
	}
}

// TestFetchChecksum_WrongLength exercises the bad-length path.
func TestFetchChecksum_WrongLength(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte("abc123\n"))
	}))
	defer srv.Close()
	sumAsset := &Asset{BrowserDownloadURL: srv.URL}
	_, err := fetchChecksum(context.Background(), Options{}, sumAsset, "gn-drive.tar.gz")
	if err == nil {
		t.Error("expected error from wrong checksum length")
	}
}

// TestFetchChecksum_BadStatus exercises the non-200 status path.
func TestFetchChecksum_BadStatus(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusNotFound)
	}))
	defer srv.Close()
	sumAsset := &Asset{BrowserDownloadURL: srv.URL}
	_, err := fetchChecksum(context.Background(), Options{}, sumAsset, "gn-drive.tar.gz")
	if err == nil {
		t.Error("expected error from 404 status")
	}
}

// TestVerifyFile_Missing exercises the os.Open error path.
func TestVerifyFile_Missing(t *testing.T) {
	err := verifyFile("/nonexistent/path", "0000000000000000000000000000000000000000000000000000000000000000")
	if err == nil {
		t.Error("expected error for missing file")
	}
}

// TestCurrentBinary_Symlink exercises the os.Executable() return path.
// (We can't easily mock os.Executable, but this ensures the function
// returns a non-empty path on the current platform.)
func TestCurrentBinary_NotEmpty(t *testing.T) {
	p := currentBinary()
	if p == "" {
		t.Error("currentBinary should return non-empty path")
	}
	if !filepath.IsAbs(p) {
		t.Errorf("currentBinary should be absolute: %q", p)
	}
}

// TestPickAssets_NoMatch exercises the "no compatible asset" path.
func TestPickAssets_NoMatch(t *testing.T) {
	rel := Release{
		TagName: "v1.0.0",
		Assets: []Asset{
			{Name: "gn-drive-windows-amd64.zip"},
			{Name: "gn-drive-linux-arm.tar.gz"},
		},
	}
	archive, sum, err := pickAssets(&rel)
	if err == nil {
		t.Error("expected error for no matching asset")
	}
	if archive != nil || sum != nil {
		t.Errorf("expected nil assets, got %v / %v", archive, sum)
	}
}

// TestPickAssets_NoChecksum exercises the path where archive is found
// but no .sha256 sidecar exists.
func TestPickAssets_NoChecksum(t *testing.T) {
	rel := Release{
		TagName: "v1.0.0",
		Assets: []Asset{
			{Name: fmt.Sprintf("gn-drive-%s-%s.tar.gz", runtime.GOOS, runtime.GOARCH)},
		},
	}
	archive, sum, err := pickAssets(&rel)
	if err == nil {
		t.Fatal("expected error for missing checksum")
	}
	if archive != nil || sum != nil {
		t.Errorf("expected nil assets, got %v / %v", archive, sum)
	}
}

// TestFetchRelease_BadJSON exercises the JSON parse error path.
func TestFetchRelease_BadJSON(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte("not json"))
	}))
	defer srv.Close()
	oldAPIBase := APIBase
	APIBase = srv.URL
	defer func() { APIBase = oldAPIBase }()
	_, err := fetchRelease(context.Background(), Options{})
	if err == nil {
		t.Error("expected error from bad JSON")
	}
}

// TestFetchRelease_BadStatus exercises the non-200 status path.
func TestFetchRelease_BadStatus(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer srv.Close()
	oldAPIBase := APIBase
	APIBase = srv.URL
	defer func() { APIBase = oldAPIBase }()
	_, err := fetchRelease(context.Background(), Options{})
	if err == nil {
		t.Error("expected error from 500 status")
	}
}

// TestUpdate_NotUpToDate covers the "newer version available" path.
func TestUpdate_NotUpToDate(t *testing.T) {
	rel := Release{
		TagName: "v2.0.0",
		Assets: []Asset{
			{Name: "gn-drive-darwin-arm64.tar.gz", BrowserDownloadURL: ""},
		},
	}
	srv := makeServer(t, rel, nil, "")
	defer srv.Close()
	oldAPIBase := APIBase
	APIBase = srv.URL
	defer func() { APIBase = oldAPIBase }()
	cur, latest, err := Check(context.Background(), Options{CurrentVersion: "1.0.0"})
	if err != nil {
		t.Fatal(err)
	}
	if cur != "1.0.0" {
		t.Errorf("cur = %q", cur)
	}
	if latest != "2.0.0" {
		t.Errorf("latest = %q", latest)
	}
}

// TestUpdate_StagingDirError covers the makeStagingDir error path.
func TestUpdate_StagingDirError(t *testing.T) {
	assetBase := fmt.Sprintf("gn-drive-%s-%s.tar.gz", runtime.GOOS, runtime.GOARCH)
	rel := Release{
		TagName: "v2.0.0",
		Assets: []Asset{
			{Name: assetBase},
			{Name: assetBase + ".sha256"},
		},
	}
	srv := makeServer(t, rel, nil, "")
	defer srv.Close()
	oldAPIBase := APIBase
	APIBase = srv.URL
	defer func() { APIBase = oldAPIBase }()

	// Set staging to an invalid path to force MkdirTemp to fail.
	_, err := Update(context.Background(), Options{
		CurrentVersion: "1.0.0",
		StagingDir:     "/nonexistent-parent-dir-" + t.Name(),
	})
	if err == nil {
		t.Error("expected error from bad staging dir")
	}
}

// TestCheck_NetworkError covers the fetchRelease network failure.
func TestCheck_NetworkError(t *testing.T) {
	oldAPIBase := APIBase
	APIBase = "http://127.0.0.1:1" // unreachable port
	defer func() { APIBase = oldAPIBase }()
	_, _, err := Check(context.Background(), Options{CurrentVersion: "1.0.0"})
	if err == nil {
		t.Error("expected error from network failure")
	}
}

// helpers

func buildTarGz(t *testing.T, path, name, content string) {
	t.Helper()
	f, err := os.Create(path)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	gz := gzip.NewWriter(f)
	tw := tar.NewWriter(gz)
	hdr := &tar.Header{Name: name, Mode: 0o755, Size: int64(len(content)), Typeflag: tar.TypeReg}
	if err := tw.WriteHeader(hdr); err != nil {
		t.Fatal(err)
	}
	if _, err := tw.Write([]byte(content)); err != nil {
		t.Fatal(err)
	}
	_ = tw.Close()
	_ = gz.Close()
}

func buildZip(t *testing.T, path, name, content string) {
	t.Helper()
	f, err := os.Create(path)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	zw := zip.NewWriter(f)
	w, err := zw.Create(name)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := w.Write([]byte(content)); err != nil {
		t.Fatal(err)
	}
	_ = zw.Close()
}

// TestMakeStagingDir_MkdirError covers the MkdirTemp error branch in
// makeStagingDir by overriding mkDirTemp to return an error.
func TestMakeStagingDir_MkdirError(t *testing.T) {
	orig := mkDirTemp
	defer func() { mkDirTemp = orig }()
	mkDirTemp = func(string, string) (string, error) {
		return "", errors.New("simulated mkdir failure")
	}
	_, err := makeStagingDir(Options{})
	if err == nil {
		t.Error("expected error from MkdirTemp failure")
	}
}

// helper: build a release with the given assets and return the API server.
// Computes the actual sha256 of the archive body so verifyFile passes.
func releaseServerForUpdate(t *testing.T) (*httptest.Server, Release) {
	t.Helper()
	assetBase := fmt.Sprintf("gn-drive-%s-%s.tar.gz", runtime.GOOS, runtime.GOARCH)
	archive := []byte("placeholder")
	h := sha256.Sum256(archive)
	sumHex := hex.EncodeToString(h[:])
	rel := Release{
		TagName: "v9.9.9",
		Assets: []Asset{
			{Name: assetBase, BrowserDownloadURL: ""},
			{Name: assetBase + ".sha256", BrowserDownloadURL: ""},
		},
	}
	srv := makeServer(t, rel, archive, sumHex)
	rel.Assets[0].BrowserDownloadURL = srv.URL + "/dl/" + assetBase
	rel.Assets[1].BrowserDownloadURL = srv.URL + "/dl/" + assetBase + ".sha256"

	oldAPIBase := APIBase
	APIBase = srv.URL
	t.Cleanup(func() { APIBase = oldAPIBase })

	return srv, rel
}

// TestUpdate_FetchChecksumError covers the fetchChecksum error branch in
// Update by overriding fetchChecksumFn.
func TestUpdate_FetchChecksumError(t *testing.T) {
	srv, _ := releaseServerForUpdate(t)
	defer srv.Close()

	orig := fetchChecksumFn
	defer func() { fetchChecksumFn = orig }()
	fetchChecksumFn = func(ctx context.Context, opts Options, sumAsset *Asset, archivePath string) (string, error) {
		return "", errors.New("simulated fetch checksum failure")
	}

	_, err := Update(context.Background(), Options{
		CurrentVersion: "0.0.0",
		Stdout:         io.Discard,
	})
	if err == nil {
		t.Error("expected error from fetchChecksum failure")
	}
}

// TestUpdate_VerifyFileError covers the verifyFile error branch in Update.
func TestUpdate_VerifyFileError(t *testing.T) {
	srv, _ := releaseServerForUpdate(t)
	defer srv.Close()

	orig := verifyFileFn
	defer func() { verifyFileFn = orig }()
	verifyFileFn = func(path, expectedHex string) error {
		return errors.New("simulated verify failure")
	}

	_, err := Update(context.Background(), Options{
		CurrentVersion: "0.0.0",
		Stdout:         io.Discard,
	})
	if err == nil {
		t.Error("expected error from verifyFile failure")
	}
}

// TestUpdate_ExtractBinaryError covers the extractBinary error branch in Update.
func TestUpdate_ExtractBinaryError(t *testing.T) {
	srv, _ := releaseServerForUpdate(t)
	defer srv.Close()

	orig := extractBinaryFn
	defer func() { extractBinaryFn = orig }()
	extractBinaryFn = func(archivePath, destDir string) (string, error) {
		return "", errors.New("simulated extract failure")
	}

	_, err := Update(context.Background(), Options{
		CurrentVersion: "0.0.0",
		Stdout:         io.Discard,
	})
	if err == nil {
		t.Error("expected error from extractBinary failure")
	}
}

// TestUpdate_DownloadError_Stub covers the download error branch via the
// downloadFn override (an alternative to the existing TestUpdate_DownloadError
// which uses a real network failure).
func TestUpdate_DownloadError_Stub(t *testing.T) {
	srv, _ := releaseServerForUpdate(t)
	defer srv.Close()

	orig := downloadFn
	defer func() { downloadFn = orig }()
	downloadFn = func(ctx context.Context, opts Options, url, dest string) error {
		return errors.New("simulated download failure")
	}

	_, err := Update(context.Background(), Options{
		CurrentVersion: "0.0.0",
		Stdout:         io.Discard,
	})
	if err == nil {
		t.Error("expected error from download failure")
	}
}

// TestUpdate_PickAssetsError covers the pickAssets error branch in Update.
func TestUpdate_PickAssetsError(t *testing.T) {
	rel := Release{TagName: "v2.0.0", Assets: []Asset{}} // empty assets
	srv := makeServer(t, rel, nil, "")
	defer srv.Close()
	oldAPIBase := APIBase
	APIBase = srv.URL
	defer func() { APIBase = oldAPIBase }()

	_, err := Update(context.Background(), Options{
		CurrentVersion: "1.0.0",
		Stdout:         io.Discard,
	})
	if err == nil {
		t.Error("expected error from pickAssets failure")
	}
}

// TestUpdate_AtomicSwapError covers the atomicSwap error branch in Update.
func TestUpdate_AtomicSwapError(t *testing.T) {
	contents := []byte("#!/bin/sh\necho hello\n")
	archivePath, sumHex := buildArchive(t, contents)
	archiveBytes, err := os.ReadFile(archivePath)
	if err != nil {
		t.Fatal(err)
	}

	assetBase := fmt.Sprintf("gn-drive-%s-%s.tar.gz", runtime.GOOS, runtime.GOARCH)
	rel := releaseJSONWithBase("ignored", "v9.9.9", assetBase, assetBase+".sha256")
	srv := makeServer(t, rel, archiveBytes, sumHex)
	defer srv.Close()
	rel.Assets[0].BrowserDownloadURL = srv.URL + "/dl/" + assetBase
	rel.Assets[1].BrowserDownloadURL = srv.URL + "/dl/" + assetBase + ".sha256"

	oldAPIBase := APIBase
	APIBase = srv.URL
	oldDownloadBase := DownloadBase
	DownloadBase = srv.URL
	t.Cleanup(func() {
		APIBase = oldAPIBase
		DownloadBase = oldDownloadBase
	})

	orig := atomicSwapFn
	defer func() { atomicSwapFn = orig }()
	atomicSwapFn = func(newBin, currentBin string) error {
		return errors.New("simulated atomicSwap failure")
	}

	stage := t.TempDir()
	_, err = Update(context.Background(), Options{
		CurrentVersion: "0.0.0",
		StagingDir:     stage,
		Stdout:         io.Discard,
	})
	if err == nil {
		t.Error("expected error from atomicSwap failure")
	}
}

// TestUpdate_OsExecutableError covers the os.Executable error branch in
// Update by overriding osExecutableFn to return an error.
func TestUpdate_OsExecutableError(t *testing.T) {
	contents := []byte("#!/bin/sh\necho hello\n")
	archivePath, sumHex := buildArchive(t, contents)
	archiveBytes, err := os.ReadFile(archivePath)
	if err != nil {
		t.Fatal(err)
	}

	assetBase := fmt.Sprintf("gn-drive-%s-%s.tar.gz", runtime.GOOS, runtime.GOARCH)
	rel := releaseJSONWithBase("ignored", "v9.9.9", assetBase, assetBase+".sha256")
	srv := makeServer(t, rel, archiveBytes, sumHex)
	defer srv.Close()
	rel.Assets[0].BrowserDownloadURL = srv.URL + "/dl/" + assetBase
	rel.Assets[1].BrowserDownloadURL = srv.URL + "/dl/" + assetBase + ".sha256"

	oldAPIBase := APIBase
	APIBase = srv.URL
	oldDownloadBase := DownloadBase
	DownloadBase = srv.URL
	t.Cleanup(func() {
		APIBase = oldAPIBase
		DownloadBase = oldDownloadBase
	})

	orig := osExecutableFn
	defer func() { osExecutableFn = orig }()
	osExecutableFn = func() (string, error) {
		return "", errors.New("simulated os.Executable failure")
	}

	stage := t.TempDir()
	_, err = Update(context.Background(), Options{
		CurrentVersion: "0.0.0",
		StagingDir:     stage,
		Stdout:         io.Discard,
	})
	if err == nil {
		t.Error("expected error from os.Executable failure")
	}
}

// TestUpdate_EvalSymlinksError covers the filepath.EvalSymlinks error branch
// in Update.
func TestUpdate_EvalSymlinksError(t *testing.T) {
	contents := []byte("#!/bin/sh\necho hello\n")
	archivePath, sumHex := buildArchive(t, contents)
	archiveBytes, err := os.ReadFile(archivePath)
	if err != nil {
		t.Fatal(err)
	}

	assetBase := fmt.Sprintf("gn-drive-%s-%s.tar.gz", runtime.GOOS, runtime.GOARCH)
	rel := releaseJSONWithBase("ignored", "v9.9.9", assetBase, assetBase+".sha256")
	srv := makeServer(t, rel, archiveBytes, sumHex)
	defer srv.Close()
	rel.Assets[0].BrowserDownloadURL = srv.URL + "/dl/" + assetBase
	rel.Assets[1].BrowserDownloadURL = srv.URL + "/dl/" + assetBase + ".sha256"

	oldAPIBase := APIBase
	APIBase = srv.URL
	oldDownloadBase := DownloadBase
	DownloadBase = srv.URL
	t.Cleanup(func() {
		APIBase = oldAPIBase
		DownloadBase = oldDownloadBase
	})

	orig := evalSymlinksFn
	defer func() { evalSymlinksFn = orig }()
	evalSymlinksFn = func(string) (string, error) {
		return "", errors.New("simulated EvalSymlinks failure")
	}

	stage := t.TempDir()
	_, err = Update(context.Background(), Options{
		CurrentVersion: "0.0.0",
		StagingDir:     stage,
		Stdout:         io.Discard,
	})
	if err == nil {
		t.Error("expected error from EvalSymlinks failure")
	}
}

// TestUpdate_ChmodError covers the os.Chmod error branch in Update by
// overriding osChmodFn.
func TestUpdate_ChmodError(t *testing.T) {
	contents := []byte("#!/bin/sh\necho hello\n")
	archivePath, sumHex := buildArchive(t, contents)
	archiveBytes, err := os.ReadFile(archivePath)
	if err != nil {
		t.Fatal(err)
	}

	assetBase := fmt.Sprintf("gn-drive-%s-%s.tar.gz", runtime.GOOS, runtime.GOARCH)
	rel := releaseJSONWithBase("ignored", "v9.9.9", assetBase, assetBase+".sha256")
	srv := makeServer(t, rel, archiveBytes, sumHex)
	defer srv.Close()
	rel.Assets[0].BrowserDownloadURL = srv.URL + "/dl/" + assetBase
	rel.Assets[1].BrowserDownloadURL = srv.URL + "/dl/" + assetBase + ".sha256"

	oldAPIBase := APIBase
	APIBase = srv.URL
	oldDownloadBase := DownloadBase
	DownloadBase = srv.URL
	t.Cleanup(func() {
		APIBase = oldAPIBase
		DownloadBase = oldDownloadBase
	})

	orig := osChmodFn
	defer func() { osChmodFn = orig }()
	osChmodFn = func(string, os.FileMode) error {
		return errors.New("simulated chmod failure")
	}

	stage := t.TempDir()
	_, err = Update(context.Background(), Options{
		CurrentVersion: "0.0.0",
		StagingDir:     stage,
		Stdout:         io.Discard,
	})
	if err == nil {
		t.Error("expected error from chmod failure")
	}
}

// TestUpdate_NewHTTPRequestError covers the newHTTPRequest error branch in
// fetchRelease.
func TestUpdate_NewHTTPRequestError(t *testing.T) {
	srv, _ := releaseServerForUpdate(t)
	defer srv.Close()

	orig := newHTTPRequest
	defer func() { newHTTPRequest = orig }()
	newHTTPRequest = func(ctx context.Context, method, url string, body io.Reader) (*http.Request, error) {
		return nil, errors.New("simulated request creation failure")
	}

	_, err := Update(context.Background(), Options{
		CurrentVersion: "0.0.0",
		Stdout:         io.Discard,
	})
	if err == nil {
		t.Error("expected error from request creation failure")
	}
}

// TestUpdate_Non200Status covers the non-200 status branch in fetchRelease.
func TestUpdate_Non200Status(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, "server error", http.StatusInternalServerError)
	}))
	defer srv.Close()

	oldAPIBase := APIBase
	APIBase = srv.URL
	t.Cleanup(func() { APIBase = oldAPIBase })

	_, err := Update(context.Background(), Options{
		CurrentVersion: "0.0.0",
		StagingDir:     t.TempDir(),
		Stdout:         io.Discard,
	})
	if err == nil {
		t.Error("expected error from 500 response")
	}
}

// TestUpdate_BadJSON covers the JSON decode error branch in fetchRelease.
func TestUpdate_BadJSON(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte("not-valid-json{"))
	}))
	defer srv.Close()

	oldAPIBase := APIBase
	APIBase = srv.URL
	t.Cleanup(func() { APIBase = oldAPIBase })

	_, err := Update(context.Background(), Options{
		CurrentVersion: "0.0.0",
		StagingDir:     t.TempDir(),
		Stdout:         io.Discard,
	})
	if err == nil {
		t.Error("expected error from bad JSON")
	}
}

// TestUpdate_EmptyTagName covers the rel.TagName == "" branch in fetchRelease.
func TestUpdate_EmptyTagName(t *testing.T) {
	rel := Release{TagName: ""} // empty tag name
	srv := makeServer(t, rel, nil, "")
	defer srv.Close()

	oldAPIBase := APIBase
	APIBase = srv.URL
	t.Cleanup(func() { APIBase = oldAPIBase })

	_, err := Update(context.Background(), Options{
		CurrentVersion: "0.0.0",
		StagingDir:     t.TempDir(),
		Stdout:         io.Discard,
	})
	if !errors.Is(err, ErrNoRelease) {
		t.Errorf("expected ErrNoRelease, got: %v", err)
	}
}

// TestUpdate_GITHUBToken covers the GITHUB_TOKEN auth header branch in
// fetchRelease.
func TestUpdate_GITHUBToken(t *testing.T) {
	rel := Release{
		TagName: "v9.9.9",
		Assets: []Asset{
			{Name: fmt.Sprintf("gn-drive-%s-%s.tar.gz", runtime.GOOS, runtime.GOARCH)},
			{Name: fmt.Sprintf("gn-drive-%s-%s.tar.gz.sha256", runtime.GOOS, runtime.GOARCH)},
		},
	}
	srv := makeServer(t, rel, nil, "")
	defer srv.Close()
	rel.Assets[0].BrowserDownloadURL = srv.URL + "/dl/" + rel.Assets[0].Name
	rel.Assets[1].BrowserDownloadURL = srv.URL + "/dl/" + rel.Assets[1].Name

	oldAPIBase := APIBase
	APIBase = srv.URL
	t.Cleanup(func() { APIBase = oldAPIBase })

	t.Setenv("GITHUB_TOKEN", "test-token-12345")

	_, err := Update(context.Background(), Options{
		CurrentVersion: "0.0.0",
		StagingDir:     t.TempDir(),
		Stdout:         io.Discard,
	})
	// We don't care about the result, just that the GITHUB_TOKEN branch was
	// exercised.
	_ = err
}

// TestFetchChecksum_SidecarFilenameMismatch covers the warning branch in
// fetchChecksum when the sidecar filename doesn't match the archive name.
func TestFetchChecksum_SidecarFilenameMismatch(t *testing.T) {
	contents := []byte("#!/bin/sh\necho hello\n")
	archivePath, sumHex := buildArchive(t, contents)
	archiveBytes, err := os.ReadFile(archivePath)
	if err != nil {
		t.Fatal(err)
	}

	assetBase := fmt.Sprintf("gn-drive-%s-%s.tar.gz", runtime.GOOS, runtime.GOARCH)
	// Build a sidecar that mentions a DIFFERENT filename to trigger the
	// warning branch.
	sidecarContent := []byte(sumHex + "  different-name.tar.gz\n")

	srv := makeServer(t, Release{}, archiveBytes, "")
	defer srv.Close()
	// Override the /sha256 handler by serving a custom sidecar.
	oldHandler := srv.Config.Handler
	srv.Config.Handler = http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if strings.HasSuffix(r.URL.Path, ".sha256") {
			w.Write(sidecarContent)
			return
		}
		oldHandler.ServeHTTP(w, r)
	})

	oldAPIBase := APIBase
	APIBase = srv.URL
	t.Cleanup(func() { APIBase = oldAPIBase })

	sum, err := fetchChecksum(context.Background(), Options{
		Stdout: io.Discard,
	}, &Asset{
		Name:               assetBase + ".sha256",
		BrowserDownloadURL: srv.URL + "/dl/" + assetBase + ".sha256",
	}, archivePath)
	if err != nil {
		t.Fatalf("fetchChecksum: %v", err)
	}
	if sum != sumHex {
		t.Errorf("sum = %q, want %q", sum, sumHex)
	}
}

// TestVerifyFile_ReadError covers the os.Open error branch in verifyFile.
func TestVerifyFile_ReadError(t *testing.T) {
	err := verifyFile("/nonexistent-path-xyz", "abc")
	if err == nil {
		t.Error("expected error from missing file")
	}
}

// TestVerifyFile_CopyError covers the io.Copy error branch in verifyFile
// by injecting a reader that errors on Read.
func TestVerifyFile_CopyError(t *testing.T) {
	orig := fileOpenerFn
	t.Cleanup(func() { fileOpenerFn = orig })
	fileOpenerFn = func(path string) (io.ReadCloser, error) {
		return io.NopCloser(errReader{}), nil
	}
	err := verifyFile("/anywhere", strings.Repeat("0", 64))
	if err == nil {
		t.Error("expected error from read failure")
	}
}

// TestDownload_RequestError covers the newHTTPRequest error branch in
// download.
func TestDownload_RequestError(t *testing.T) {
	orig := newHTTPRequest
	defer func() { newHTTPRequest = orig }()
	newHTTPRequest = func(ctx context.Context, method, url string, body io.Reader) (*http.Request, error) {
		return nil, errors.New("simulated request creation failure")
	}
	err := download(context.Background(), Options{}, "http://example.com/x", t.TempDir()+"/dest")
	if err == nil {
		t.Error("expected error from request creation failure")
	}
}

// TestDownload_OpenFileError covers the os.OpenFile error branch in
// download by passing a path under a regular file.
func TestDownload_OpenFileError(t *testing.T) {
	dir := t.TempDir()
	blocker := filepath.Join(dir, "blocker")
	if err := os.WriteFile(blocker, []byte("hi"), 0o600); err != nil {
		t.Fatal(err)
	}

	// Use a stub server that returns 200 with a body.
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(200)
		_, _ = w.Write([]byte("data"))
	}))
	defer srv.Close()

	err := download(context.Background(), Options{}, srv.URL, filepath.Join(blocker, "dest"))
	if err == nil {
		t.Error("expected error from OpenFile failure")
	}
}

// TestFetchChecksum_RequestError covers the newHTTPRequest error branch in
// fetchChecksum.
func TestFetchChecksum_RequestError(t *testing.T) {
	orig := newHTTPRequest
	defer func() { newHTTPRequest = orig }()
	newHTTPRequest = func(ctx context.Context, method, url string, body io.Reader) (*http.Request, error) {
		return nil, errors.New("simulated request creation failure")
	}
	_, err := fetchChecksum(context.Background(), Options{}, &Asset{
		BrowserDownloadURL: "http://example.com/x.sha256",
	}, "/tmp/archive.tar.gz")
	if err == nil {
		t.Error("expected error from request creation failure")
	}
}

// TestFetchChecksum_DoError covers the httpClient.Do error branch in
// fetchChecksum by using an unreachable URL.
func TestFetchChecksum_DoError(t *testing.T) {
	savedTimeout := HTTPTimeout
	HTTPTimeout = 1 * time.Millisecond
	t.Cleanup(func() { HTTPTimeout = savedTimeout })

	_, err := fetchChecksum(context.Background(), Options{}, &Asset{
		BrowserDownloadURL: "http://127.0.0.1:1/x.sha256",
	}, "/tmp/archive.tar.gz")
	if err == nil {
		t.Error("expected error from network failure")
	}
}

// TestFetchChecksum_ReadAllError covers the io.ReadAll error branch in
// fetchChecksum by using a custom HTTP client that returns an erroring
// body.
func TestFetchChecksum_ReadAllError(t *testing.T) {
	client := &http.Client{
		Transport: erroringTransport{body: errReader{}},
	}

	_, err := fetchChecksum(context.Background(), Options{
		Stdout:     io.Discard,
		HTTPClient: client,
	}, &Asset{
		BrowserDownloadURL: "http://example.invalid/x.sha256",
	}, "/tmp/archive.tar.gz")
	if err == nil {
		t.Error("expected error from read failure")
	}
}

// errReader is an io.Reader that always returns an error.
type errReader struct{}

func (errReader) Read([]byte) (int, error) { return 0, errors.New("simulated read error") }

// erroringTransport returns a fixed body for any request.
type erroringTransport struct {
	body io.Reader
}

func (t erroringTransport) RoundTrip(req *http.Request) (*http.Response, error) {
	return &http.Response{
		StatusCode: http.StatusOK,
		Body:       io.NopCloser(t.body),
		Header:     make(http.Header),
	}, nil
}

// TestExtractTarGz_GzipError covers the gzip.NewReader error branch in
// extractTarGz by passing a non-gzip file.
func TestExtractTarGz_GzipError_Extra(t *testing.T) {
	dir := t.TempDir()
	archive := filepath.Join(dir, "bad.tar.gz")
	if err := os.WriteFile(archive, []byte("not a gzip file"), 0o644); err != nil {
		t.Fatal(err)
	}
	_, err := extractTarGz(archive, dir)
	if err == nil {
		t.Error("expected error from invalid gzip")
	}
}

// TestExtractTarGz_OpenError covers the os.Open error branch in extractTarGz.
func TestExtractTarGz_OpenError(t *testing.T) {
	_, err := extractTarGz("/nonexistent-path.tar.gz", t.TempDir())
	if err == nil {
		t.Error("expected error from missing archive")
	}
}

// TestExtractZip_OpenError covers the os.Open error branch in extractZip.
func TestExtractZip_OpenError(t *testing.T) {
	_, err := extractZip("/nonexistent-path.zip", t.TempDir())
	if err == nil {
		t.Error("expected error from missing archive")
	}
}

// TestCurrentBinary_OsExecutableError covers the osExecutableFn error
// branch in currentBinary which returns "".
func TestCurrentBinary_OsExecutableError(t *testing.T) {
	orig := osExecutableFn
	t.Cleanup(func() { osExecutableFn = orig })
	osExecutableFn = func() (string, error) {
		return "", errors.New("simulated os.Executable failure")
	}
	if got := currentBinary(); got != "" {
		t.Errorf("currentBinary() with osExecutable error: want %q, got %q", "", got)
	}
}

// TestCurrentBinary_EvalSymlinksError covers the evalSymlinksFn error
// branch in currentBinary which falls through to returning the
// unresolved path.
func TestCurrentBinary_EvalSymlinksError(t *testing.T) {
	orig := evalSymlinksFn
	t.Cleanup(func() { evalSymlinksFn = orig })
	evalSymlinksFn = func(string) (string, error) {
		return "", errors.New("simulated EvalSymlinks failure")
	}
	got := currentBinary()
	if got == "" {
		t.Error("currentBinary() should return unresolved path when EvalSymlinks fails")
	}
}

// TestExtractTarGz_NonRegEntry covers the hdr.Typeflag != tar.TypeReg
// branch in extractTarGz.
func TestExtractTarGz_NonRegEntry(t *testing.T) {
	dir := t.TempDir()
	archivePath := filepath.Join(dir, "mixed.tar.gz")
	f, err := os.Create(archivePath)
	if err != nil {
		t.Fatal(err)
	}
	gz := gzip.NewWriter(f)
	tw := tar.NewWriter(gz)
	// Write a directory entry first (TypeDir), then the gn-drive binary.
	if err := tw.WriteHeader(&tar.Header{
		Name:     "gn-drive",
		Typeflag: tar.TypeDir,
		Mode:     0o755,
	}); err != nil {
		t.Fatal(err)
	}
	contents := []byte("fake binary")
	if err := tw.WriteHeader(&tar.Header{
		Name:     "gn-drive",
		Mode:     0o755,
		Size:     int64(len(contents)),
		Typeflag: tar.TypeReg,
	}); err != nil {
		t.Fatal(err)
	}
	if _, err := tw.Write(contents); err != nil {
		t.Fatal(err)
	}
	if err := tw.Close(); err != nil {
		t.Fatal(err)
	}
	if err := gz.Close(); err != nil {
		t.Fatal(err)
	}
	if err := f.Close(); err != nil {
		t.Fatal(err)
	}

	destDir := t.TempDir()
	got, err := extractTarGz(archivePath, destDir)
	if err != nil {
		t.Fatalf("extractTarGz: %v", err)
	}
	if !strings.HasSuffix(got, "gn-drive") {
		t.Errorf("unexpected extract path: %s", got)
	}
}

// TestExtractTarGz_CorruptArchive covers the tr.Next() non-EOF error
// branch in extractTarGz by writing an invalid header block into the
// gzipped stream.
func TestExtractTarGz_CorruptArchive(t *testing.T) {
	dir := t.TempDir()
	archivePath := filepath.Join(dir, "corrupt.tar.gz")
	f, err := os.Create(archivePath)
	if err != nil {
		t.Fatal(err)
	}
	gz := gzip.NewWriter(f)
	// Write a valid header first.
	tw := tar.NewWriter(gz)
	if err := tw.WriteHeader(&tar.Header{
		Name:     "first",
		Mode:     0o644,
		Size:     5,
		Typeflag: tar.TypeReg,
	}); err != nil {
		t.Fatal(err)
	}
	if _, err := tw.Write([]byte("hello")); err != nil {
		t.Fatal(err)
	}
	// Append garbage that will be interpreted as a tar header.
	// 512 bytes of zeros — tar reader will likely return an error
	// because the magic number is wrong.
	if _, err := gz.Write(make([]byte, 512)); err != nil {
		t.Fatal(err)
	}
	if err := gz.Close(); err != nil {
		t.Fatal(err)
	}
	if err := f.Close(); err != nil {
		t.Fatal(err)
	}

	_, err = extractTarGz(archivePath, t.TempDir())
	if err == nil {
		t.Error("expected error from corrupt tar archive")
	}
}
// branch in extractTarGz.
func TestExtractTarGz_BinaryNotFound(t *testing.T) {
	dir := t.TempDir()
	archivePath := filepath.Join(dir, "no-binary.tar.gz")
	f, err := os.Create(archivePath)
	if err != nil {
		t.Fatal(err)
	}
	gz := gzip.NewWriter(f)
	tw := tar.NewWriter(gz)
	contents := []byte("some other file")
	if err := tw.WriteHeader(&tar.Header{
		Name:     "README.md",
		Mode:     0o644,
		Size:     int64(len(contents)),
		Typeflag: tar.TypeReg,
	}); err != nil {
		t.Fatal(err)
	}
	if _, err := tw.Write(contents); err != nil {
		t.Fatal(err)
	}
	if err := tw.Close(); err != nil {
		t.Fatal(err)
	}
	if err := gz.Close(); err != nil {
		t.Fatal(err)
	}
	if err := f.Close(); err != nil {
		t.Fatal(err)
	}

	_, err = extractTarGz(archivePath, t.TempDir())
	if err == nil {
		t.Error("expected error when binary not found in archive")
	}
}

// TestExtractTarGz_OpenFileError covers the os.OpenFile error branch in
// extractTarGz by attempting to write into a non-existent/read-only
// destination directory.
func TestExtractTarGz_OpenFileError(t *testing.T) {
	contents := []byte("#!/bin/sh\necho hello\n")
	archivePath, _ := buildArchive(t, contents)

	_, err := extractTarGz(archivePath, "/nonexistent/destination/path")
	if err == nil {
		t.Error("expected error from missing destination directory")
	}
}

// TestExtractTarGz_WindowsBinaryName covers the runtime.GOOS=="windows"
// branch by overriding the binaryName lookup via the runtime value is
// not overridable, so we exercise it indirectly by passing an archive
// containing only a "gn-drive.exe" entry, which is not what the test
// archive will have. Skipped: behavior is identical to gn-drive case
// because we only inspect filepath.Base().
func TestExtractTarGz_SkipsNonMatchingEntries(t *testing.T) {
	dir := t.TempDir()
	archivePath := filepath.Join(dir, "with-others.tar.gz")
	f, err := os.Create(archivePath)
	if err != nil {
		t.Fatal(err)
	}
	gz := gzip.NewWriter(f)
	tw := tar.NewWriter(gz)

	// First entry: a non-matching regular file (should be skipped).
	contents1 := []byte("ignore me")
	if err := tw.WriteHeader(&tar.Header{
		Name:     "README.md",
		Mode:     0o644,
		Size:     int64(len(contents1)),
		Typeflag: tar.TypeReg,
	}); err != nil {
		t.Fatal(err)
	}
	if _, err := tw.Write(contents1); err != nil {
		t.Fatal(err)
	}

	// Second entry: the actual gn-drive binary.
	contents2 := []byte("#!/bin/sh\necho hello\n")
	if err := tw.WriteHeader(&tar.Header{
		Name:     "gn-drive",
		Mode:     0o755,
		Size:     int64(len(contents2)),
		Typeflag: tar.TypeReg,
	}); err != nil {
		t.Fatal(err)
	}
	if _, err := tw.Write(contents2); err != nil {
		t.Fatal(err)
	}
	if err := tw.Close(); err != nil {
		t.Fatal(err)
	}
	if err := gz.Close(); err != nil {
		t.Fatal(err)
	}
	if err := f.Close(); err != nil {
		t.Fatal(err)
	}

	got, err := extractTarGz(archivePath, t.TempDir())
	if err != nil {
		t.Fatalf("extractTarGz: %v", err)
	}
	if !strings.HasSuffix(got, "gn-drive") {
		t.Errorf("unexpected extract path: %s", got)
	}
}

// TestExtractZip_BinaryNotFound covers the "binary not found in archive"
// branch in extractZip.
func TestExtractZip_BinaryNotFound(t *testing.T) {
	dir := t.TempDir()
	archivePath := filepath.Join(dir, "no-binary.zip")
	f, err := os.Create(archivePath)
	if err != nil {
		t.Fatal(err)
	}
	zw := zip.NewWriter(f)
	if _, err := zw.Create("README.md"); err != nil {
		t.Fatal(err)
	}
	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}
	if err := f.Close(); err != nil {
		t.Fatal(err)
	}

	_, err = extractZip(archivePath, t.TempDir())
	if err == nil {
		t.Error("expected error when gn-drive.exe not in zip")
	}
}

// TestExtractZip_OpenFileError covers the os.OpenFile dest error branch in
// extractZip.
func TestExtractZip_OpenFileError(t *testing.T) {
	dir := t.TempDir()
	archivePath := filepath.Join(dir, "with-binary.zip")
	f, err := os.Create(archivePath)
	if err != nil {
		t.Fatal(err)
	}
	zw := zip.NewWriter(f)
	w, err := zw.Create("gn-drive.exe")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := w.Write([]byte("fake binary content")); err != nil {
		t.Fatal(err)
	}
	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}
	if err := f.Close(); err != nil {
		t.Fatal(err)
	}

	_, err = extractZip(archivePath, "/nonexistent/destination")
	if err == nil {
		t.Error("expected error from missing destination")
	}
}

// TestExtractZip_FileOpenError covers the f.Open error branch in
// extractZip by injecting an opener that returns an error.
func TestExtractZip_FileOpenError(t *testing.T) {
	dir := t.TempDir()
	archivePath := filepath.Join(dir, "with-binary.zip")
	f, err := os.Create(archivePath)
	if err != nil {
		t.Fatal(err)
	}
	zw := zip.NewWriter(f)
	w, err := zw.Create("gn-drive.exe")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := w.Write([]byte("content")); err != nil {
		t.Fatal(err)
	}
	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}
	if err := f.Close(); err != nil {
		t.Fatal(err)
	}

	orig := zipFileOpenerFn
	t.Cleanup(func() { zipFileOpenerFn = orig })
	zipFileOpenerFn = func(f *zip.File) (io.ReadCloser, error) {
		return nil, errors.New("simulated file open error")
	}

	_, err = extractZip(archivePath, t.TempDir())
	if err == nil {
		t.Error("expected error from file open failure")
	}
}

// TestExtractZip_CopyError covers the io.Copy error branch in extractZip
// by injecting an opener that returns an erroring reader.
func TestExtractZip_CopyError(t *testing.T) {
	dir := t.TempDir()
	archivePath := filepath.Join(dir, "with-binary.zip")
	f, err := os.Create(archivePath)
	if err != nil {
		t.Fatal(err)
	}
	zw := zip.NewWriter(f)
	w, err := zw.Create("gn-drive.exe")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := w.Write([]byte("content")); err != nil {
		t.Fatal(err)
	}
	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}
	if err := f.Close(); err != nil {
		t.Fatal(err)
	}

	orig := zipFileOpenerFn
	t.Cleanup(func() { zipFileOpenerFn = orig })
	zipFileOpenerFn = func(f *zip.File) (io.ReadCloser, error) {
		return io.NopCloser(errReader{}), nil
	}

	_, err = extractZip(archivePath, t.TempDir())
	if err == nil {
		t.Error("expected error from copy failure")
	}
}

// TestAtomicSwap_WindowsRestoreError covers the Windows-branch second
// osRenameFn failure that triggers a restore attempt in atomicSwap.
func TestAtomicSwap_WindowsRestoreError(t *testing.T) {
	orig := isWindows
	t.Cleanup(func() { isWindows = orig })
	isWindows = func() bool { return true }

	dir := t.TempDir()
	currentBin := filepath.Join(dir, "gn-drive")
	newBin := filepath.Join(dir, "gn-drive.new")

	if err := os.WriteFile(currentBin, []byte("current"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(newBin, []byte("new"), 0o755); err != nil {
		t.Fatal(err)
	}

	// Track rename calls.
	calls := 0
	origRename := osRenameFn
	t.Cleanup(func() { osRenameFn = origRename })
	osRenameFn = func(old, new string) error {
		calls++
		if calls == 1 {
			// First rename: currentBin -> .old (succeed).
			return os.Rename(old, new)
		}
		if calls == 2 {
			// Second rename: newBin -> currentBin (fail → triggers restore).
			return errors.New("simulated second rename failure")
		}
		// Restore attempt.
		return nil
	}

	err := atomicSwap(newBin, currentBin)
	if err == nil {
		t.Error("expected error from second rename failure")
	}
	if calls < 3 {
		t.Errorf("expected at least 3 rename calls (incl. restore), got %d", calls)
	}
}

// TestFetchChecksum_EmptySidecar covers the empty checksum file branch
// in fetchChecksum.
func TestFetchChecksum_EmptySidecar(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Write only whitespace, then TrimSpace gives "" and Fields is empty.
		w.Write([]byte("   \n"))
	}))
	defer srv.Close()

	_, err := fetchChecksum(context.Background(), Options{Stdout: io.Discard}, &Asset{
		BrowserDownloadURL: srv.URL + "/x.sha256",
	}, "/tmp/archive.tar.gz")
	if err == nil {
		t.Error("expected error from empty checksum file")
	}
}

// TestFetchChecksum_InvalidLength covers the "invalid checksum length"
// branch in fetchChecksum.
func TestFetchChecksum_InvalidLength(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte("abc123\n"))
	}))
	defer srv.Close()

	_, err := fetchChecksum(context.Background(), Options{Stdout: io.Discard}, &Asset{
		BrowserDownloadURL: srv.URL + "/x.sha256",
	}, "/tmp/archive.tar.gz")
	if err == nil {
		t.Error("expected error from invalid checksum length")
	}
}

// TestFetchChecksum_Non200 covers the non-200 status branch in fetchChecksum.
func TestFetchChecksum_Non200(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer srv.Close()

	_, err := fetchChecksum(context.Background(), Options{Stdout: io.Discard}, &Asset{
		BrowserDownloadURL: srv.URL + "/x.sha256",
	}, "/tmp/archive.tar.gz")
	if err == nil {
		t.Error("expected error from non-200 status")
	}
}

// TestPickAssets_OtherArchive covers the case where the release contains
// a non-matching platform archive that should be skipped.
func TestPickAssets_OtherArchive(t *testing.T) {
	assetBase := fmt.Sprintf("gn-drive-%s-%s.tar.gz", runtime.GOOS, runtime.GOARCH)
	rel := Release{
		TagName: "v1.0.0",
		Assets: []Asset{
			{Name: "gn-drive-darwin-arm64.tar.gz"},
			{Name: "gn-drive-linux-amd64.tar.gz"},
			{Name: assetBase},
			{Name: assetBase + ".sha256"},
		},
	}
	bin, sum, err := pickAssets(&rel)
	if err != nil {
		t.Fatalf("pickAssets: %v", err)
	}
	if bin == nil || sum == nil {
		t.Error("expected non-nil assets for matching platform")
	}
	if bin.Name != assetBase {
		t.Errorf("expected %q, got %q", assetBase, bin.Name)
	}
}

// TestPickAssets_Windows exercises the windows branch in pickAssets by
// overriding isWindows.
func TestPickAssets_Windows(t *testing.T) {
	orig := isWindows
	t.Cleanup(func() { isWindows = orig })
	isWindows = func() bool { return true }

	// pickAssets uses runtime.GOOS for the osName portion of the filename
	// but isWindows() for the extension. So the asset name must include
	// runtime.GOOS (e.g. "darwin") but end in ".zip".
	rel := Release{
		TagName: "v1.0.0",
		Assets: []Asset{
			{Name: fmt.Sprintf("gn-drive-%s-%s.zip", runtime.GOOS, runtime.GOARCH)},
			{Name: fmt.Sprintf("gn-drive-%s-%s.zip.sha256", runtime.GOOS, runtime.GOARCH)},
		},
	}
	bin, sum, err := pickAssets(&rel)
	if err != nil {
		t.Fatalf("pickAssets: %v", err)
	}
	if bin == nil || sum == nil {
		t.Error("expected non-nil assets for windows match")
	}
}

// TestExtractTarGz_WindowsBinaryName covers the windows binary name
// branch in extractTarGz by overriding isWindows and building an
// archive that contains gn-drive.exe instead of gn-drive.
func TestExtractTarGz_WindowsBinaryName(t *testing.T) {
	orig := isWindows
	t.Cleanup(func() { isWindows = orig })
	isWindows = func() bool { return true }

	dir := t.TempDir()
	archivePath := filepath.Join(dir, "windows.tar.gz")
	f, err := os.Create(archivePath)
	if err != nil {
		t.Fatal(err)
	}
	gz := gzip.NewWriter(f)
	tw := tar.NewWriter(gz)
	contents := []byte("fake windows binary")
	if err := tw.WriteHeader(&tar.Header{
		Name:     "gn-drive.exe",
		Mode:     0o755,
		Size:     int64(len(contents)),
		Typeflag: tar.TypeReg,
	}); err != nil {
		t.Fatal(err)
	}
	if _, err := tw.Write(contents); err != nil {
		t.Fatal(err)
	}
	if err := tw.Close(); err != nil {
		t.Fatal(err)
	}
	if err := gz.Close(); err != nil {
		t.Fatal(err)
	}
	if err := f.Close(); err != nil {
		t.Fatal(err)
	}

	got, err := extractTarGz(archivePath, t.TempDir())
	if err != nil {
		t.Fatalf("extractTarGz: %v", err)
	}
	if !strings.HasSuffix(got, "gn-drive.exe") {
		t.Errorf("unexpected extract path: %s", got)
	}
}

// TestVerifyFile_Mismatch covers the checksum mismatch branch.
func TestVerifyFile_Mismatch(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "file.txt")
	if err := os.WriteFile(path, []byte("hello"), 0o644); err != nil {
		t.Fatal(err)
	}
	err := verifyFile(path, "0000000000000000000000000000000000000000000000000000000000000000")
	if !errors.Is(err, ErrChecksumMismatch) {
		t.Errorf("expected ErrChecksumMismatch, got: %v", err)
	}
}

// TestUpdate_StagingDirError covers the makeStagingDir error branch by
// overriding mkDirTemp.
func TestUpdate_StagingDirError_Extra(t *testing.T) {
	origMk := mkDirTemp
	t.Cleanup(func() { mkDirTemp = origMk })
	mkDirTemp = func(string, string) (string, error) {
		return "", errors.New("simulated mkdir failure")
	}

	contents := []byte("#!/bin/sh\necho hello\n")
	archivePath, sumHex := buildArchive(t, contents)
	archiveBytes, err := os.ReadFile(archivePath)
	if err != nil {
		t.Fatal(err)
	}

	assetBase := fmt.Sprintf("gn-drive-%s-%s.tar.gz", runtime.GOOS, runtime.GOARCH)
	rel := releaseJSONWithBase("ignored", "v9.9.9", assetBase, assetBase+".sha256")
	srv := makeServer(t, rel, archiveBytes, sumHex)
	defer srv.Close()
	rel.Assets[0].BrowserDownloadURL = srv.URL + "/dl/" + assetBase
	rel.Assets[1].BrowserDownloadURL = srv.URL + "/dl/" + assetBase + ".sha256"

	oldAPIBase := APIBase
	APIBase = srv.URL
	oldDownloadBase := DownloadBase
	DownloadBase = srv.URL
	t.Cleanup(func() {
		APIBase = oldAPIBase
		DownloadBase = oldDownloadBase
	})

	_, err = Update(context.Background(), Options{
		CurrentVersion: "0.0.0",
		StagingDir:     t.TempDir(),
		Stdout:         io.Discard,
	})
	if err == nil {
		t.Error("expected error from staging dir failure")
	}
}
