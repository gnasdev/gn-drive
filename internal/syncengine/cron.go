package syncengine

import (
	"fmt"
	"strings"

	"github.com/robfig/cron/v3"
)

// secondAwareParser matches Engine's cron.WithSeconds() configuration.
var secondAwareParser = cron.NewParser(
	cron.Second | cron.Minute | cron.Hour | cron.Dom | cron.Month | cron.Dow | cron.Descriptor,
)

// NormalizeCron accepts either a standard 5-field expression (min hour dom mon dow)
// as used by the web UI, or a 6-field expression with seconds (as used by the
// engine's cron.WithSeconds parser). Returns a 6-field expression ready for
// AddFunc, or an error if the expression is invalid.
func NormalizeCron(expr string) (string, error) {
	expr = strings.TrimSpace(expr)
	if expr == "" {
		return "", fmt.Errorf("cron expression is empty")
	}
	// Descriptor forms like "@hourly" work with the seconds-aware parser as-is.
	if strings.HasPrefix(expr, "@") {
		if _, err := secondAwareParser.Parse(expr); err != nil {
			return "", fmt.Errorf("invalid cron expression %q: %w", expr, err)
		}
		return expr, nil
	}
	fields := strings.Fields(expr)
	switch len(fields) {
	case 5:
		// UI default "0 * * * *" (every hour at minute 0) → "0 0 * * * *"
		expr = "0 " + expr
	case 6:
		// already second-aware
	default:
		return "", fmt.Errorf("invalid cron expression %q: want 5 or 6 fields, got %d", expr, len(fields))
	}
	if _, err := secondAwareParser.Parse(expr); err != nil {
		return "", fmt.Errorf("invalid cron expression %q: %w", expr, err)
	}
	return expr, nil
}
