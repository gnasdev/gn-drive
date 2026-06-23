package selfupdate

import (
	"archive/tar"
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
