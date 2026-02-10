package utils

import (
	"context"
	"desktop/backend/dto"
	"encoding/hex"
	"errors"
	"fmt"
	"hash"
	"io"
	"os"
	"os/exec"
	"runtime"
	"runtime/pprof"
	"strconv"
	"time"

	"github.com/rclone/rclone/cmd"
	"github.com/rclone/rclone/fs"
	"github.com/rclone/rclone/fs/accounting"
	"github.com/rclone/rclone/lib/atexit"
)

func resolveError(ctx context.Context, err error) error {
	ci := fs.GetConfig(ctx)
	atexit.Run()

	if err == nil {
		if ci.ErrorOnNoTransfer {
			if accounting.Stats(ctx).GetTransfers() == 0 {
				return errors.New("no files transferred")
			}
		}
		return nil
	}

	return err
}

func RunRcloneWithRetryAndStats(ctx context.Context, retry bool, showStats bool, outStatus chan *dto.SyncStatusDTO, cb func() error) error {
	var cmdErr error

	fsConfig := fs.GetConfig(ctx)
	stats := accounting.Stats(ctx)

	stopStats := func() {}
	if !showStats && cmd.ShowStats() {
		showStats = true
	}

	stopStats = startProgress(ctx, outStatus)

	cmd.SigInfoHandler()

	for try := 1; try <= fsConfig.Retries; try++ {
		cmdErr = cb()
		cmdErr = fs.CountError(ctx, cmdErr)
		lastErr := stats.GetLastError()
		if cmdErr == nil {
			cmdErr = lastErr
		}
		if !retry || !stats.Errored() {
			if try > 1 {
				fs.Errorf(nil, "Attempt %d/%d succeeded", try, fsConfig.Retries)
			}
			break
		}
		if stats.HadFatalError() {
			fs.Errorf(nil, "Fatal error received - not attempting retries")
			break
		}
		if stats.Errored() && !stats.HadRetryError() {
			fs.Errorf(nil, "Can't retry any of the errors - not attempting retries")
			break
		}
		if retryAfter := stats.RetryAfter(); !retryAfter.IsZero() {
			d := time.Until(retryAfter)
			if d > 0 {
				fs.Logf(nil, "Received retry after error - sleeping until %s (%v)", retryAfter.Format(time.RFC3339Nano), d)
				time.Sleep(d)
			}
		}
		if lastErr != nil {
			fs.Errorf(nil, "Attempt %d/%d failed with %d errors and: %v", try, fsConfig.Retries, stats.GetErrors(), lastErr)
		} else {
			fs.Errorf(nil, "Attempt %d/%d failed with %d errors", try, fsConfig.Retries, stats.GetErrors())
		}
		if try < fsConfig.Retries {
			stats.ResetErrors()
		}
		if fsConfig.RetriesInterval > 0 {
			time.Sleep(time.Duration(fsConfig.RetriesInterval))
		}
	}

	stopStats()
	if showStats && stats.Errored() {
		stats.Log()
	}

	fs.Debugf(nil, "%d go routines active\n", runtime.NumGoroutine())

	// dump all running go-routines
	if fsConfig.Dump&fs.DumpGoRoutines != 0 {
		err := pprof.Lookup("goroutine").WriteTo(os.Stdout, 1)
		if err != nil {
			fs.Errorf(nil, "Failed to dump goroutines: %v", err)
		}
	}

	// dump open files
	if fsConfig.Dump&fs.DumpOpenFiles != 0 {
		c := exec.Command("lsof", "-p", strconv.Itoa(os.Getpid()))
		c.Stdout = os.Stdout
		c.Stderr = os.Stderr
		err := c.Run()
		if err != nil {
			fs.Errorf(nil, "Failed to list open files: %v", err)
		}
	}

	if lastErr := stats.GetLastError(); cmdErr == nil {
		cmdErr = lastErr
	}

	// Log the final error message and exit
	if cmdErr != nil {
		nerrs := stats.GetErrors()
		if nerrs <= 1 {
			fs.Logf(nil, "Failed: %v", cmdErr)
		} else {
			fs.Logf(nil, "Failed with %d errors: last error was: %v", nerrs, cmdErr)
		}
	}

	return resolveError(ctx, cmdErr)
}

func CalculateFileHash(filePath string, hashFunc func() hash.Hash) (string, error) {
	// Open the file
	file, err := os.Open(filePath)
	if err != nil {
		return "", fmt.Errorf("failed to open file: %w", err)
	}
	defer func() {
		if err := file.Close(); err != nil {
			fmt.Printf("Warning: failed to close file: %v\n", err)
		}
	}()

	// Initialize the hash function
	hasher := hashFunc()

	// Copy the file content to the hasher
	if _, err := io.Copy(hasher, file); err != nil {
		return "", fmt.Errorf("failed to hash file: %w", err)
	}

	// Compute the final hash
	hashSum := hasher.Sum(nil)

	// Return the hash as a hexadecimal string
	return hex.EncodeToString(hashSum), nil
}

func CalculateContentHash(content []byte, hashFunc func() hash.Hash) (string, error) {
	// Initialize the hash function
	hasher := hashFunc()

	// Write the content to the hasher
	if _, err := hasher.Write(content); err != nil {
		return "", fmt.Errorf("failed to hash content: %w", err)
	}

	// Compute the final hash
	hashSum := hasher.Sum(nil)

	// Return the hash as a hexadecimal string
	return hex.EncodeToString(hashSum), nil
}

func MergeBytes(a, b []byte) []byte {
	merged := make([]byte, len(a)+len(b))
	copy(merged, a)
	copy(merged[len(a):], b)
	return merged
}
