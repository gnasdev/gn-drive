package webui

import (
	"errors"
	"io"
	"io/fs"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestHandler_ReturnsNonNil(t *testing.T) {
	h := Handler()
	if h == nil {
		t.Fatal("Handler returned nil")
	}
}

func TestHandler_ServesIndex(t *testing.T) {
	h := Handler()
	req := httptest.NewRequest("GET", "/", nil)
	rr := httptest.NewRecorder()
	h.ServeHTTP(rr, req)
	if rr.Code != http.StatusOK {
		t.Errorf("status = %d, want 200", rr.Code)
	}
	body, _ := io.ReadAll(rr.Body)
	if !strings.Contains(string(body), "<") {
		t.Errorf("body does not look like HTML: %q", body)
	}
}

func TestHandler_ServesAssets(t *testing.T) {
	// Open the embedded dist/ to find an asset path.
	entries, err := distFS.ReadDir("dist")
	if err != nil {
		t.Fatal(err)
	}
	// Look for an assets dir.
	var assetPath string
	for _, e := range entries {
		if e.IsDir() && e.Name() == "assets" {
			assets, _ := distFS.ReadDir("dist/assets")
			if len(assets) > 0 {
				assetPath = "/assets/" + assets[0].Name()
				break
			}
		}
	}
	if assetPath == "" {
		t.Skip("no assets embedded")
	}
	h := Handler()
	req := httptest.NewRequest("GET", assetPath, nil)
	rr := httptest.NewRecorder()
	h.ServeHTTP(rr, req)
	if rr.Code != http.StatusOK {
		t.Errorf("status for %s = %d, want 200", assetPath, rr.Code)
	}
}

func TestHandler_SPAFallback(t *testing.T) {
	// Any path that doesn't exist as a file should fall back to index.html.
	h := Handler()
	req := httptest.NewRequest("GET", "/some/spa/route/that/does/not/exist", nil)
	rr := httptest.NewRecorder()
	h.ServeHTTP(rr, req)
	if rr.Code != http.StatusOK {
		t.Errorf("status = %d, want 200 (SPA fallback)", rr.Code)
	}
	body, _ := io.ReadAll(rr.Body)
	if !strings.Contains(string(body), "<") {
		t.Errorf("SPA fallback body should be HTML: %q", body)
	}
}

func TestHandler_RootHasContent(t *testing.T) {
	h := Handler()
	req := httptest.NewRequest("GET", "/", nil)
	rr := httptest.NewRecorder()
	h.ServeHTTP(rr, req)
	body, _ := io.ReadAll(rr.Body)
	if len(body) == 0 {
		t.Error("expected non-empty body for /")
	}
}

// TestHandler_SubFSError covers the panic branch when subFS returns an
// error. We override the package var to inject the error.
func TestHandler_SubFSError(t *testing.T) {
	orig := subFS
	defer func() { subFS = orig }()
	subFS = func(fsys fs.FS, dir string) (fs.FS, error) {
		return nil, errors.New("simulated missing dist")
	}

	defer func() {
		if r := recover(); r == nil {
			t.Error("expected panic when subFS errors")
		}
	}()
	_ = Handler()
}
