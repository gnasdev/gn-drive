package store

import (
	"context"
	"io"
	"log/slog"
	"path/filepath"
	"testing"
)

func migDiscardLogger() *slog.Logger {
	return slog.New(slog.NewTextHandler(io.Discard, nil))
}

// TestStore_StampsSchemaVersion verifies New stamps PRAGMA user_version to the
// current schemaVersion and that re-running migrate is idempotent.
func TestStore_StampsSchemaVersion(t *testing.T) {
	ctx := context.Background()
	st, err := New(ctx, filepath.Join(t.TempDir(), "db.db"), migDiscardLogger())
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()

	var v int
	if err := st.DB().QueryRowContext(ctx, "PRAGMA user_version").Scan(&v); err != nil {
		t.Fatal(err)
	}
	if v != schemaVersion {
		t.Errorf("user_version = %d, want %d", v, schemaVersion)
	}

	// Re-running migrate must not error and must keep the version stable.
	if err := st.migrate(ctx); err != nil {
		t.Fatalf("re-migrate: %v", err)
	}
	if err := st.DB().QueryRowContext(ctx, "PRAGMA user_version").Scan(&v); err != nil {
		t.Fatal(err)
	}
	if v != schemaVersion {
		t.Errorf("after re-migrate user_version = %d, want %d", v, schemaVersion)
	}
}

// TestApplyMigrations_RunsPending verifies a pending (higher-version) migration
// is executed and stamps the new version.
func TestApplyMigrations_RunsPending(t *testing.T) {
	ctx := context.Background()
	st, err := New(ctx, filepath.Join(t.TempDir(), "db.db"), migDiscardLogger())
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()

	orig := migrations
	defer func() { migrations = orig }()
	migrations = append(append([]migration{}, orig...),
		migration{version: 99, sql: "CREATE TABLE migration_probe (id TEXT)"})

	if err := st.applyMigrations(ctx); err != nil {
		t.Fatalf("applyMigrations: %v", err)
	}

	var v int
	if err := st.DB().QueryRowContext(ctx, "PRAGMA user_version").Scan(&v); err != nil {
		t.Fatal(err)
	}
	if v != 99 {
		t.Errorf("user_version = %d, want 99", v)
	}
	if _, err := st.DB().ExecContext(ctx, "INSERT INTO migration_probe (id) VALUES ('x')"); err != nil {
		t.Errorf("probe table not created by migration: %v", err)
	}

	// Idempotent: applying again is a no-op (no duplicate-table error).
	if err := st.applyMigrations(ctx); err != nil {
		t.Errorf("re-applyMigrations should be a no-op: %v", err)
	}
}
