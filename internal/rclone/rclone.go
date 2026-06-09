// Package rclone provides a shell-out wrapper around the rclone binary.
//
// Phase 2: implements sync, bisync, copy, move, check, list, mkdir, purge,
// delete, about, and remote CRUD via exec.Command("rclone", ...). Progress is
// reported via a simple stats channel populated from rclone's --stats output.
//
// The wrapper does not depend on the rclone Go library — keeping go.mod minimal
// and ensuring any rclone version installed on the host works.
package rclone

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
)

// Action represents a sync direction.
type Action string

const (
	ActionPull      Action = "pull"
	ActionPush      Action = "push"
	ActionBi        Action = "bi"
	ActionBiResync  Action = "bi-resync"
	ActionCopy      Action = "copy"
	ActionMove      Action = "move"
	ActionCheck     Action = "check"
	ActionDryRun    Action = "dry-run"
)

// Client wraps the rclone binary and config path.
type Client struct {
	mu        sync.Mutex
	rcloneBin string
	config    string
	logger    *slog.Logger
}

// Options configures the Client.
type Options struct {
	// BinaryPath is the absolute path to rclone. Defaults to "rclone" in PATH.
	BinaryPath string
	// ConfigPath is the rclone.conf path. Defaults to ~/.config/gn-drive/rclone.conf.
	ConfigPath string
	// Logger is the structured logger to use. Defaults to slog.Default().
	Logger *slog.Logger
}

// New creates a new rclone Client.
func New(opts Options) (*Client, error) {
	bin := opts.BinaryPath
	if bin == "" {
		p, err := exec.LookPath("rclone")
		if err != nil {
			return nil, fmt.Errorf("rclone: binary not found in PATH: %w", err)
		}
		bin = p
	} else {
		if _, err := os.Stat(bin); err != nil {
			return nil, fmt.Errorf("rclone: binary not found at %s: %w", bin, err)
		}
	}
	logger := opts.Logger
	if logger == nil {
		logger = slog.Default()
	}
	return &Client{
		rcloneBin: bin,
		config:    opts.ConfigPath,
		logger:    logger,
	}, nil
}

// Binary returns the resolved rclone binary path.
func (c *Client) Binary() string { return c.rcloneBin }

// ConfigPath returns the rclone.conf path used by this client.
func (c *Client) ConfigPath() string { return c.config }

// Version returns the rclone version string.
func (c *Client) Version(ctx context.Context) (string, error) {
	out, err := c.run(ctx, nil, "version")
	if err != nil {
		return "", err
	}
	// First line: "rclone v1.74.2"
	first := strings.SplitN(string(out), "\n", 2)[0]
	return strings.TrimSpace(first), nil
}

// Stats describes progress during a sync operation.
type Stats struct {
	Bytes          int64   `json:"bytes"`
	BytesTotal     int64   `json:"bytes_total"`
	Files          int64   `json:"files"`
	FilesTotal     int64   `json:"files_total"`
	Transfers      int64   `json:"transfers"`
	Errors         int64   `json:"errors"`
	Checks         int64   `json:"checks"`
	ChecksTotal    int64   `json:"checks_total"`
	Deletes        int64   `json:"deletes"`
	Speed          float64 `json:"speed_bps"`
	ETA            int64   `json:"eta_secs"`
	CurrentFile    string  `json:"current_file,omitempty"`
	LastUpdate     int64   `json:"last_update_unix"`
}

// SyncResult is the outcome of a sync operation.
type SyncResult struct {
	Stats     Stats
	StartedAt int64
	EndedAt   int64
	ExitCode  int
	Stderr    string
}

// SyncConfig is the per-operation configuration.
type SyncConfig struct {
	Action       Action
	Source       string // remote:path or local path
	SourceRemote string
	SourcePath   string
	Dest         string
	DestRemote   string
	DestPath     string
	// Resync forces a bisync resync.
	Resync bool
	// Profile is the optional profile to apply flags from.
	Profile *ProfileFlags
	// StatsInterval is how often to emit stats. Default: 1s.
	StatsInterval string
}

// ProfileFlags are the rclone flags a profile can set.
// Mirrors a subset of store.Profile relevant to runtime.
type ProfileFlags struct {
	Bandwidth         string
	Transfers         int
	Checkers          int
	TpsLimit          float64
	MinAge            string
	MaxAge            string
	MinSize           string
	MaxSize           string
	ExcludeIfPresent  string
	MaxDelete         int
	DryRun            bool
	UseListR          bool
	NoUnicodeNormalize bool
}

// --- Sync / BiSync / Copy / Move / Check ----------------------------------

// Sync runs the configured action. It streams progress via onProgress (may be nil).
func (c *Client) Sync(ctx context.Context, cfg SyncConfig, onProgress func(Stats)) (*SyncResult, error) {
	args, cleanup, err := c.buildArgs(cfg)
	if err != nil {
		return nil, err
	}
	if cleanup != "" {
		defer os.Remove(cleanup)
	}
	return c.execute(ctx, args, onProgress, cfg.StatsInterval)
}

func (c *Client) buildArgs(cfg SyncConfig) (args []string, cleanup string, err error) {
	src, dst, err := c.resolveEndpoints(cfg)
	if err != nil {
		return nil, "", err
	}

	interval := cfg.StatsInterval
	if interval == "" {
		interval = "1s"
	}

	base := []string{"--config", c.config, "--stats", interval, "--stats-one-line", "-v"}

	switch cfg.Action {
	case ActionPull:
		args = append([]string{"sync", src, dst, "--update"}, base...)
	case ActionPush:
		args = append([]string{"sync", dst, src, "--update"}, base...)
	case ActionBi:
		args = append([]string{"bisync", src, dst, "--resync"}, base...)
		cleanup = filepath.Join(os.TempDir(), fmt.Sprintf("gn-drive-resync-%d", os.Getpid()))
		_ = os.WriteFile(cleanup, []byte{}, 0o600)
		args = append(args, "--resync-mode-path", cleanup)
	case ActionBiResync:
		args = append([]string{"bisync", src, dst, "--resync", "--force"}, base...)
	case ActionCopy:
		args = append([]string{"copy", src, dst}, base...)
	case ActionMove:
		args = append([]string{"move", src, dst}, base...)
	case ActionCheck:
		args = append([]string{"check", src, dst}, base...)
	case ActionDryRun:
		args = append([]string{"sync", src, dst, "--dry-run", "--update"}, base...)
	default:
		return nil, "", fmt.Errorf("rclone: unknown action %q", cfg.Action)
	}

	if cfg.Profile != nil {
		args = append(args, profileToFlags(cfg.Profile)...)
	}
	return args, cleanup, nil
}

func (c *Client) resolveEndpoints(cfg SyncConfig) (src, dst string, err error) {
	if cfg.Source != "" && cfg.Dest != "" {
		return cfg.Source, cfg.Dest, nil
	}
	if cfg.SourceRemote == "" || cfg.SourcePath == "" || cfg.DestRemote == "" || cfg.DestPath == "" {
		return "", "", errors.New("rclone: SyncConfig requires Source+Dest or SourceRemote+SourcePath+DestRemote+DestPath")
	}
	return cfg.SourceRemote + ":" + cfg.SourcePath, cfg.DestRemote + ":" + cfg.DestPath, nil
}

func profileToFlags(p *ProfileFlags) []string {
	if p == nil {
		return nil
	}
	var f []string
	if p.Bandwidth != "" {
		f = append(f, "--bwlimit", p.Bandwidth)
	}
	if p.Transfers > 0 {
		f = append(f, "--transfers", strconv.Itoa(p.Transfers))
	}
	if p.Checkers > 0 {
		f = append(f, "--checkers", strconv.Itoa(p.Checkers))
	}
	if p.TpsLimit > 0 {
		f = append(f, "--tpslimit", strconv.FormatFloat(p.TpsLimit, 'f', -1, 64))
	}
	if p.MinAge != "" {
		f = append(f, "--min-age", p.MinAge)
	}
	if p.MaxAge != "" {
		f = append(f, "--max-age", p.MaxAge)
	}
	if p.MinSize != "" {
		f = append(f, "--min-size", p.MinSize)
	}
	if p.MaxSize != "" {
		f = append(f, "--max-size", p.MaxSize)
	}
	if p.ExcludeIfPresent != "" {
		f = append(f, "--exclude-if-present", p.ExcludeIfPresent)
	}
	if p.MaxDelete > 0 {
		f = append(f, "--max-delete", strconv.Itoa(p.MaxDelete))
	}
	if p.DryRun {
		f = append(f, "--dry-run")
	}
	if p.NoUnicodeNormalize {
		f = append(f, "--no-unicode-normalization")
	}
	return f
}

// execute runs rclone with the given args and parses --stats-one-line output.
func (c *Client) execute(ctx context.Context, args []string, onProgress func(Stats), _ string) (*SyncResult, error) {
	c.mu.Lock()
	cmd := exec.CommandContext(ctx, c.rcloneBin, args...)
	c.mu.Unlock()

	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return nil, fmt.Errorf("rclone: stdout pipe: %w", err)
	}
	stderr, err := cmd.StderrPipe()
	if err != nil {
		return nil, fmt.Errorf("rclone: stderr pipe: %w", err)
	}

	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("rclone: start: %w", err)
	}

	result := &SyncResult{StartedAt: nowUnix(), ExitCode: -1}

	// Drain stderr.
	var stderrBuf strings.Builder
	stderrDone := make(chan struct{})
	go func() {
		_, _ = io.Copy(&stderrBuf, stderr)
		close(stderrDone)
	}()

	// Parse --stats-one-line: lines like
	//   "2025/01/15 10:00:00 INFO  : ... TRANSFER: 1.024k/2.048k ..."
	// We extract numbers opportunistically; --json-stats would be richer
	// but is only available in rclone 1.61+ and adds noise.
	stats := Stats{}
	scanner := bufio.NewScanner(stdout)
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)
	go func() {
		for scanner.Scan() {
			line := scanner.Text()
			parseStatsLine(line, &stats)
			if onProgress != nil {
				stats.LastUpdate = nowUnix()
				onProgress(stats)
			}
		}
	}()

	<-stderrDone
	if err := cmd.Wait(); err != nil {
		var exitErr *exec.ExitError
		if errors.As(err, &exitErr) {
			result.ExitCode = exitErr.ExitCode()
		}
		result.Stderr = stderrBuf.String()
		result.EndedAt = nowUnix()
		result.Stats = stats
		return result, fmt.Errorf("rclone: %w (stderr: %s)", err, truncate(stderrBuf.String(), 500))
	}

	result.ExitCode = 0
	result.EndedAt = nowUnix()
	result.Stats = stats
	return result, nil
}

// parseStatsLine extracts progress numbers from an rclone --stats-one-line line.
// Format (approximate): "2025/01/15 10:00:00 INFO  : ... TRANSFER: 1.024k/2.048k ..."
func parseStatsLine(line string, s *Stats) {
	if !strings.Contains(line, "INFO") {
		return
	}
	// Look for "X/Y" patterns after TRANSFER / CHECK / etc.
	if i := strings.Index(line, "TRANSFER: "); i >= 0 {
		if a, b, ok := parseFraction(line[i:]); ok {
			s.Bytes = a
			s.BytesTotal = b
		}
	}
	if i := strings.Index(line, "CHECK: "); i >= 0 {
		if a, b, ok := parseFraction(line[i:]); ok {
			s.Checks = a
			s.ChecksTotal = b
		}
	}
	if i := strings.Index(line, "ERRORS: "); i >= 0 {
		if n, ok := parseInt(line[i:]); ok {
			s.Errors = n
		}
	}
	if i := strings.Index(line, "DELETED: "); i >= 0 {
		if n, ok := parseInt(line[i:]); ok {
			s.Deletes = n
		}
	}
}

func parseFraction(s string) (int64, int64, bool) {
	// Find "X/Y" where X and Y are size-suffixed numbers (e.g. "1.024k/2.048k").
	idx := strings.Index(s, " ")
	if idx < 0 {
		return 0, 0, false
	}
	rest := s[idx+1:]
	slash := strings.Index(rest, "/")
	if slash < 0 {
		return 0, 0, false
	}
	left := rest[:slash]
	// Take the next token after "/"
	rightAndMore := rest[slash+1:]
	space := strings.Index(rightAndMore, " ")
	var right string
	if space < 0 {
		right = rightAndMore
	} else {
		right = rightAndMore[:space]
	}
	return parseSize(left), parseSize(right), true
}

func parseInt(s string) (int64, bool) {
	idx := strings.Index(s, " ")
	if idx < 0 {
		return 0, false
	}
	rest := s[idx+1:]
	end := 0
	for end < len(rest) && (rest[end] >= '0' && rest[end] <= '9') {
		end++
	}
	if end == 0 {
		return 0, false
	}
	n, err := strconv.ParseInt(rest[:end], 10, 64)
	if err != nil {
		return 0, false
	}
	return n, true
}

// parseSize parses rclone size suffixes: "1.024k", "2M", "1G", "1024".
// Returns bytes.
func parseSize(s string) int64 {
	if s == "" {
		return 0
	}
	// Find first non-digit/dot character.
	i := 0
	for i < len(s) && (s[i] >= '0' && s[i] <= '9' || s[i] == '.') {
		i++
	}
	numStr := s[:i]
	suffix := strings.ToLower(s[i:])
	if numStr == "" {
		return 0
	}
	n, err := strconv.ParseFloat(numStr, 64)
	if err != nil {
		return 0
	}
	mult := float64(1)
	switch suffix {
	case "k", "kb":
		mult = 1024
	case "m", "mb":
		mult = 1024 * 1024
	case "g", "gb":
		mult = 1024 * 1024 * 1024
	case "t", "tb":
		mult = 1024 * 1024 * 1024 * 1024
	}
	return int64(n * mult)
}

func nowUnix() int64 {
	return timeNowFunc()
}

// --- File operations -------------------------------------------------------

// ListFiles returns entries at a remote path.
func (c *Client) ListFiles(ctx context.Context, remotePath string) ([]FileEntry, error) {
	if !strings.Contains(remotePath, ":") {
		return nil, errors.New("rclone: remote path must include remote name (e.g. \"gdrive:/folder\")")
	}
	out, err := c.run(ctx, nil, "lsjson", remotePath, "--config", c.config, "--files-only")
	if err != nil {
		return nil, err
	}
	var entries []FileEntry
	if err := json.Unmarshal(out, &entries); err != nil {
		return nil, fmt.Errorf("rclone: parse lsjson: %w", err)
	}
	return entries, nil
}

// Mkdir creates a directory on a remote.
func (c *Client) Mkdir(ctx context.Context, remotePath string) error {
	_, err := c.run(ctx, nil, "mkdir", remotePath, "--config", c.config)
	return err
}

// Purge removes a directory and all its contents.
func (c *Client) Purge(ctx context.Context, remotePath string) error {
	_, err := c.run(ctx, nil, "purge", remotePath, "--config", c.config)
	return err
}

// DeleteFile deletes a single file.
func (c *Client) DeleteFile(ctx context.Context, remotePath string) error {
	_, err := c.run(ctx, nil, "deletefile", remotePath, "--config", c.config)
	return err
}

// About returns quota info for a remote (no path).
func (c *Client) About(ctx context.Context, remoteName string) (*QuotaInfo, error) {
	out, err := c.run(ctx, nil, "about", remoteName+":", "--config", c.config, "--json")
	if err != nil {
		return nil, err
	}
	var a struct {
		Used  int64 `json:"used"`
		Total int64 `json:"total"`
		Free  int64 `json:"free"`
	}
	if err := json.Unmarshal(out, &a); err != nil {
		return nil, fmt.Errorf("rclone: parse about: %w", err)
	}
	return &QuotaInfo{Used: a.Used, Total: a.Total, Free: a.Free}, nil
}

// --- Remotes CRUD ---------------------------------------------------------

// Remote describes an rclone remote.
type Remote struct {
	Name        string `json:"name"`
	Type        string `json:"type"`
	Description string `json:"description,omitempty"`
}

// ListRemotes returns all remotes in rclone.conf.
func (c *Client) ListRemotes(ctx context.Context) ([]Remote, error) {
	// rclone config listremotes (no --long flag; format: "remote:")
	// Exit 2 + Usage message when config is empty/missing — treat as zero remotes.
	out, err := c.run(ctx, nil, "config", "listremotes", "--config", c.config)
	if err != nil {
		outStr := string(out)
		if strings.Contains(outStr, "Usage:") || strings.Contains(outStr, "Available commands:") {
			return nil, nil
		}
		return nil, err
	}
	var remotes []Remote
	for _, line := range strings.Split(strings.TrimSpace(string(out)), "\n") {
		if line == "" {
			continue
		}
		name := strings.TrimSuffix(strings.TrimSpace(line), ":")
		if name == "" {
			continue
		}
		remotes = append(remotes, Remote{Name: name, Type: ""})
	}
	return remotes, nil
}

// CreateRemote creates a new remote non-interactively.
// configKVs is a list of "key=value" pairs to pass to rclone config create.
func (c *Client) CreateRemote(ctx context.Context, name, remoteType string, configKVs []string) error {
	args := []string{"config", "create", name, remoteType}
	for _, kv := range configKVs {
		args = append(args, kv)
	}
	args = append(args, "--config", c.config)
	_, err := c.run(ctx, nil, args...)
	return err
}

// DeleteRemote removes a remote.
func (c *Client) DeleteRemote(ctx context.Context, name string) error {
	_, err := c.run(ctx, nil, "config", "delete", name, "--config", c.config)
	return err
}

// TestRemote verifies that the remote is reachable by listing its root.
func (c *Client) TestRemote(ctx context.Context, name string) error {
	_, err := c.run(ctx, nil, "lsd", name+":", "--config", c.config, "--max-depth", "1")
	return err
}

// --- internal -------------------------------------------------------------

func (c *Client) run(ctx context.Context, env []string, args ...string) ([]byte, error) {
	c.mu.Lock()
	cmd := exec.CommandContext(ctx, c.rcloneBin, args...)
	c.mu.Unlock()
	cmd.Env = append(os.Environ(), env...)
	out, err := cmd.CombinedOutput()
	if err != nil {
		return out, fmt.Errorf("rclone %s: %w (%s)", strings.Join(args, " "), err, truncate(string(out), 500))
	}
	return out, nil
}

func truncate(s string, n int) string {
	if len(s) <= n {
		return s
	}
	return s[:n] + "...(truncated)"
}
