// Package webui serves the embedded Vue SPA (//go:embed all:dist).
package webui

import (
	"embed"
	"io/fs"
	"net/http"
)

//go:embed all:dist
var distFS embed.FS

// subFS is overridable for tests; defaults to fs.Sub.
var subFS = fs.Sub

// Handler returns an http.Handler that serves the embedded dist/ directory.
// Unknown paths fall back to index.html for SPA routing.
func Handler() http.Handler {
	sub, err := subFS(distFS, "dist")
	if err != nil {
		// dist/ should always exist; this is a build-time error.
		panic("webui: dist/ not embedded: " + err.Error())
	}
	files := http.FS(sub)
	return spaHandler{files: files, fs: sub}
}

type spaHandler struct {
	files http.FileSystem
	fs    fs.FS
}

func (h spaHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	// Try the requested path first.
	path, err := h.fs.Open(r.URL.Path[1:]) // strip leading /
	if err == nil {
		_ = path.Close()
		http.FileServer(h.files).ServeHTTP(w, r)
		return
	}
	// SPA fallback: serve index.html.
	r.URL.Path = "/"
	http.FileServer(h.files).ServeHTTP(w, r)
}
