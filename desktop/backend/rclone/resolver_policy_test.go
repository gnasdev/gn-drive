package rclone

import (
	"desktop/backend/models"
	"testing"
)

func TestBuildResolverPolicyLocalLocalKeepsUserParallelism(t *testing.T) {
	policy := buildResolverPolicy(models.Profile{
		From:     "/Users/me/source",
		To:       "/Users/me/destination",
		Parallel: 12,
	}, nil)

	if policy.Transfers != 12 {
		t.Fatalf("transfers = %d, want 12", policy.Transfers)
	}
	if policy.Checkers != 12 {
		t.Fatalf("checkers = %d, want 12", policy.Checkers)
	}
	if !policy.UseListR {
		t.Fatal("UseListR = false, want true")
	}
}

func TestBuildResolverPolicyDriveToLocalIsConservative(t *testing.T) {
	policy := buildResolverPolicy(models.Profile{
		From:     "gdrive:/Team",
		To:       "/Users/me/Team",
		Parallel: 16,
	}, map[string]string{"gdrive": "drive"})

	if policy.Transfers != 4 {
		t.Fatalf("transfers = %d, want 4", policy.Transfers)
	}
	if policy.Checkers != 4 {
		t.Fatalf("checkers = %d, want 4", policy.Checkers)
	}
	if policy.TPSLimit != 4 {
		t.Fatalf("TPSLimit = %.1f, want 4.0", policy.TPSLimit)
	}
	if policy.RetriesSleep == 0 {
		t.Fatal("RetriesSleep = 0, want a cloud retry interval")
	}
}

func TestBuildResolverPolicyCloudCloudLimitsResolvingFanout(t *testing.T) {
	policy := buildResolverPolicy(models.Profile{
		From:     "gdrive:/Team",
		To:       "dropbox:/Team",
		Parallel: 10,
	}, map[string]string{"gdrive": "drive", "dropbox": "dropbox"})

	if policy.Transfers != 2 {
		t.Fatalf("transfers = %d, want 2", policy.Transfers)
	}
	if policy.Checkers != 2 {
		t.Fatalf("checkers = %d, want 2", policy.Checkers)
	}
	if policy.TPSLimit != 4 {
		t.Fatalf("TPSLimit = %.1f, want most conservative provider limit 4.0", policy.TPSLimit)
	}
}

func TestBuildResolverPolicyScopedCloudDisablesListR(t *testing.T) {
	policy := buildResolverPolicy(models.Profile{
		From:          "gdrive:/Team",
		To:            "/Users/me/Team",
		Parallel:      8,
		IncludedPaths: []string{"/Reports/**"},
	}, map[string]string{"gdrive": "drive"})

	if policy.UseListR {
		t.Fatal("UseListR = true, want false for scoped cloud resolving")
	}
	if policy.Checkers != 2 {
		t.Fatalf("checkers = %d, want scoped cloud cap 2", policy.Checkers)
	}
}

func TestRemoteNameIgnoresLocalPathsWithColonInDirectory(t *testing.T) {
	if _, ok := remoteName("/tmp/a:b/file.txt"); ok {
		t.Fatal("remoteName treated a local path with a colon after slash as remote")
	}
	if _, ok := remoteName(`C:\Users\me\file.txt`); ok {
		t.Fatal("remoteName treated a Windows drive path as remote")
	}
	if name, ok := remoteName("gdrive:/folder"); !ok || name != "gdrive" {
		t.Fatalf("remoteName = %q, %t; want gdrive, true", name, ok)
	}
}
