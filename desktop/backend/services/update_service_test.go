package services

import "testing"

func TestIsVersionNewer(t *testing.T) {
	tests := []struct {
		name      string
		candidate string
		current   string
		want      bool
		wantErr   bool
	}{
		{name: "newer patch", candidate: "v1.2.4", current: "1.2.3", want: true},
		{name: "newer minor", candidate: "1.3.0", current: "1.2.9", want: true},
		{name: "same version", candidate: "v1.2.3", current: "1.2.3", want: false},
		{name: "older version", candidate: "1.2.2", current: "1.2.3", want: false},
		{name: "invalid candidate", candidate: "latest", current: "1.2.3", wantErr: true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := isVersionNewer(tt.candidate, tt.current)
			if tt.wantErr {
				if err == nil {
					t.Fatalf("expected error")
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if got != tt.want {
				t.Fatalf("isVersionNewer() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestSelectUpdateAssetRequiresChecksum(t *testing.T) {
	assets := []githubReleaseAsset{
		{Name: "gn-drive-darwin-arm64.zip", BrowserDownloadURL: "https://example.test/app.zip"},
		{Name: "gn-drive-darwin-arm64.zip.sha256", BrowserDownloadURL: "https://example.test/app.zip.sha256"},
		{Name: "gn-drive-linux-amd64.tar.gz", BrowserDownloadURL: "https://example.test/app.tar.gz"},
	}

	asset, checksum, ok := selectUpdateAsset(assets, "darwin", "arm64")
	if !ok {
		t.Fatalf("expected darwin arm64 asset")
	}
	if asset.Name != "gn-drive-darwin-arm64.zip" {
		t.Fatalf("asset = %q", asset.Name)
	}
	if checksum.Name != "gn-drive-darwin-arm64.zip.sha256" {
		t.Fatalf("checksum = %q", checksum.Name)
	}

	_, _, ok = selectUpdateAsset(assets, "linux", "amd64")
	if ok {
		t.Fatalf("expected linux asset without checksum to be rejected")
	}
}

func TestSafeArchivePathRejectsTraversal(t *testing.T) {
	tests := []struct {
		name string
		path string
		ok   bool
	}{
		{name: "regular file", path: "gn-drive.app/Contents/MacOS/gn-drive", ok: true},
		{name: "parent traversal", path: "../gn-drive", ok: false},
		{name: "nested traversal", path: "gn-drive/../../bad", ok: false},
		{name: "absolute path", path: "/tmp/gn-drive", ok: false},
		{name: "empty path", path: "", ok: false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := safeArchivePath("/tmp/updates", tt.path)
			if tt.ok && err != nil {
				t.Fatalf("expected path to be accepted: %v", err)
			}
			if !tt.ok && err == nil {
				t.Fatalf("expected path to be rejected")
			}
		})
	}
}

func TestParseSHA256(t *testing.T) {
	hash := "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
	got, err := parseSHA256(hash+"  gn-drive-darwin-arm64.zip\n", "gn-drive-darwin-arm64.zip")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got != hash {
		t.Fatalf("hash = %q", got)
	}
}
