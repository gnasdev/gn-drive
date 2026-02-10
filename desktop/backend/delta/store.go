package delta

import (
	"database/sql"
	"time"
)

// DeltaStore provides CRUD operations for delta state in SQLite.
type DeltaStore struct {
	getDB func() (*sql.DB, error)
}

// NewDeltaStore creates a DeltaStore with the given DB accessor function.
func NewDeltaStore(getDB func() (*sql.DB, error)) *DeltaStore {
	return &DeltaStore{getDB: getDB}
}

// GetState returns the delta state for a remote endpoint, or nil if not found.
func (s *DeltaStore) GetState(remoteKey string) (*DeltaState, error) {
	db, err := s.getDB()
	if err != nil {
		return nil, err
	}

	row := db.QueryRow(`
		SELECT remote_key, provider, is_watching, last_full_sync, delta_count
		FROM delta_state WHERE remote_key = ?`, remoteKey)

	state := &DeltaState{}
	var lastFullSync sql.NullString
	var isWatching int

	err = row.Scan(&state.RemoteKey, &state.Provider, &isWatching, &lastFullSync, &state.DeltaCount)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}

	state.IsWatching = isWatching != 0
	if lastFullSync.Valid {
		t, err := time.Parse(time.RFC3339, lastFullSync.String)
		if err == nil {
			state.LastFullSync = &t
		}
	}

	return state, nil
}

// RecordFullSync records a full sync completion: resets delta_count, sets last_full_sync,
// and updates provider and watching state.
func (s *DeltaStore) RecordFullSync(remoteKey, provider string, isWatching bool) error {
	db, err := s.getDB()
	if err != nil {
		return err
	}

	now := time.Now().UTC().Format(time.RFC3339)
	watchInt := 0
	if isWatching {
		watchInt = 1
	}

	_, err = db.Exec(`
		INSERT INTO delta_state (remote_key, provider, is_watching, last_full_sync, delta_count, updated_at)
		VALUES (?, ?, ?, ?, 0, ?)
		ON CONFLICT(remote_key) DO UPDATE SET
			provider = excluded.provider,
			is_watching = excluded.is_watching,
			last_full_sync = excluded.last_full_sync,
			delta_count = 0,
			updated_at = excluded.updated_at`,
		remoteKey, provider, watchInt, now, now)
	return err
}

// IncrementDeltaCount increments the delta sync counter for a remote endpoint.
func (s *DeltaStore) IncrementDeltaCount(remoteKey string) error {
	db, err := s.getDB()
	if err != nil {
		return err
	}

	now := time.Now().UTC().Format(time.RFC3339)
	_, err = db.Exec(`
		UPDATE delta_state SET delta_count = delta_count + 1, updated_at = ?
		WHERE remote_key = ?`, now, remoteKey)
	return err
}

// SetWatching updates the is_watching flag for a remote endpoint.
func (s *DeltaStore) SetWatching(remoteKey string, watching bool) error {
	db, err := s.getDB()
	if err != nil {
		return err
	}

	watchInt := 0
	if watching {
		watchInt = 1
	}
	now := time.Now().UTC().Format(time.RFC3339)
	_, err = db.Exec(`
		UPDATE delta_state SET is_watching = ?, updated_at = ?
		WHERE remote_key = ?`, watchInt, now, remoteKey)
	return err
}
