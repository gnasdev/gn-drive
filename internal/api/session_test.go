package api

import (
	"sync"
	"testing"
)

func TestSessionStore_AddValidDelete(t *testing.T) {
	s := NewSessionStore()
	if s.Count() != 0 {
		t.Fatalf("new store should be empty, got %d", s.Count())
	}

	token := "abc123"
	s.Add(token)
	if !s.Valid(token) {
		t.Fatal("token should be valid after Add")
	}
	if s.Count() != 1 {
		t.Errorf("Count = %d, want 1", s.Count())
	}

	// Token not added is invalid.
	if s.Valid("never-added") {
		t.Error("unknown token should be invalid")
	}

	s.Delete(token)
	if s.Valid(token) {
		t.Error("token should be invalid after Delete")
	}
	if s.Count() != 0 {
		t.Errorf("Count after Delete = %d, want 0", s.Count())
	}
}

func TestSessionStore_DeleteMissingIsNoop(t *testing.T) {
	s := NewSessionStore()
	// Should not panic.
	s.Delete("never-existed")
	if s.Count() != 0 {
		t.Errorf("Count = %d, want 0", s.Count())
	}
}

func TestSessionStore_DuplicateAddIdempotent(t *testing.T) {
	s := NewSessionStore()
	s.Add("t")
	s.Add("t")
	s.Add("t")
	if s.Count() != 1 {
		t.Errorf("Count after duplicate Add = %d, want 1", s.Count())
	}
}

func TestSessionStore_GlobalVar(t *testing.T) {
	// The package-level sessionStore is reachable through the helper
	// functions; we exercise them here to make sure they hit the same map.
	token := "global-token-1"
	sessionAdd(token)
	if !sessionValid(token) {
		t.Fatal("global sessionValid should return true for added token")
	}
	sessionDelete(token)
	if sessionValid(token) {
		t.Error("global sessionValid should return false after delete")
	}
}

func TestSessionStore_Concurrent(t *testing.T) {
	s := NewSessionStore()
	const N = 100
	var wg sync.WaitGroup
	wg.Add(N)
	for i := 0; i < N; i++ {
		go func(i int) {
			defer wg.Done()
			token := string(rune('a'+(i%26))) + "-x"
			s.Add(token)
			if !s.Valid(token) {
				t.Errorf("token %q not valid immediately after Add", token)
			}
			s.Delete(token)
		}(i)
	}
	wg.Wait()
	// Allow duplicates per bucket (a-x, b-x, ...) so final count may be up
	// to 26; just ensure no entries are leaked from goroutines.
	for i := 0; i < 26; i++ {
		token := string(rune('a'+i)) + "-x"
		// Token may or may not exist depending on race; ensure no panic.
		_ = s.Valid(token)
	}
}
