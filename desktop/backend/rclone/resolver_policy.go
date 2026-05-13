package rclone

import (
	"context"
	"desktop/backend/models"
	"log"
	"strings"
	"time"

	"github.com/rclone/rclone/fs"
	fsConfig "github.com/rclone/rclone/fs/config"
)

type endpointKind string

const (
	endpointLocal   endpointKind = "local"
	endpointCloud   endpointKind = "cloud"
	endpointUnknown endpointKind = "unknown"
)

type endpointInfo struct {
	Kind       endpointKind
	RemoteName string
	RemoteType string
	RateRisk   bool
	Known      bool
}

type ResolverPolicy struct {
	Source           endpointInfo
	Destination      endpointInfo
	ExplicitParallel bool
	Transfers        int
	Checkers         int
	TPSLimit         float64
	UseListR         bool
	Retries          int
	LowLevelRetries  int
	RetriesSleep     time.Duration
}

func ApplyResolverPolicy(ctx context.Context, profile models.Profile) ResolverPolicy {
	policy := buildResolverPolicy(profile, configuredRemoteTypes())
	cfg := fs.GetConfig(ctx)

	if policy.ExplicitParallel && policy.Transfers > 0 {
		cfg.Transfers = policy.Transfers
	}
	if policy.ExplicitParallel && policy.Checkers > 0 {
		cfg.Checkers = policy.Checkers
	}
	if policy.TPSLimit > 0 && profile.TpsLimit == nil {
		cfg.TPSLimit = policy.TPSLimit
	}
	if policy.Retries > 0 && profile.Retries == nil {
		cfg.Retries = policy.Retries
	}
	if policy.LowLevelRetries > 0 && profile.LowLevelRetries == nil {
		cfg.LowLevelRetries = policy.LowLevelRetries
	}
	if policy.RetriesSleep > 0 && profile.RetriesSleep == "" {
		cfg.RetriesInterval = fs.Duration(policy.RetriesSleep)
	}
	cfg.UseListR = policy.UseListR

	log.Printf(
		"[resolver-policy] src=%s/%s dst=%s/%s transfers=%d checkers=%d tps=%.1f listR=%t retries=%d lowLevelRetries=%d retrySleep=%s",
		policy.Source.Kind,
		policy.Source.RemoteType,
		policy.Destination.Kind,
		policy.Destination.RemoteType,
		policy.Transfers,
		policy.Checkers,
		policy.TPSLimit,
		policy.UseListR,
		policy.Retries,
		policy.LowLevelRetries,
		policy.RetriesSleep,
	)
	return policy
}

func buildResolverPolicy(profile models.Profile, remoteTypes map[string]string) ResolverPolicy {
	source := classifyEndpoint(profile.From, remoteTypes)
	destination := classifyEndpoint(profile.To, remoteTypes)
	parallel := profile.Parallel
	explicitParallel := parallel > 0
	if parallel <= 0 {
		parallel = 4
	}

	cloudCount := 0
	rateRisk := source.RateRisk || destination.RateRisk
	unknownRemote := hasUnknownRemote(source) || hasUnknownRemote(destination)
	for _, endpoint := range []endpointInfo{source, destination} {
		if endpoint.Kind == endpointCloud || endpoint.Kind == endpointUnknown {
			cloudCount++
		}
	}

	policy := ResolverPolicy{
		Source:           source,
		Destination:      destination,
		ExplicitParallel: explicitParallel,
		Transfers:        parallel,
		Checkers:         min(parallel, 16),
		UseListR:         true,
		Retries:          3,
		LowLevelRetries:  3,
	}

	switch {
	case cloudCount == 0:
		policy.Transfers = parallel
		policy.Checkers = min(max(parallel, 4), 16)
	case cloudCount >= 2:
		policy.Transfers = min(parallel, 2)
		policy.Checkers = min(parallel, 2)
		policy.RetriesSleep = 5 * time.Second
	case rateRisk || unknownRemote:
		policy.Transfers = min(parallel, 4)
		policy.Checkers = min(parallel, 4)
		policy.RetriesSleep = 4 * time.Second
	default:
		policy.Transfers = min(parallel, 6)
		policy.Checkers = min(parallel, 6)
		policy.RetriesSleep = 3 * time.Second
	}

	if len(profile.IncludedPaths) > 0 || profile.MaxDepth != nil {
		policy.UseListR = false
		if cloudCount > 0 {
			policy.Checkers = min(policy.Checkers, 2)
		}
	}

	if defaultTPS := resolverTPSLimit(source, destination); defaultTPS > 0 {
		policy.TPSLimit = defaultTPS
	}
	return policy
}

func classifyEndpoint(path string, remoteTypes map[string]string) endpointInfo {
	remoteName, ok := remoteName(path)
	if !ok {
		return endpointInfo{Kind: endpointLocal, RemoteType: "local", Known: true}
	}

	remoteType, known := remoteTypes[strings.ToLower(remoteName)]
	if !known {
		return endpointInfo{Kind: endpointUnknown, RemoteName: remoteName, RemoteType: "unknown"}
	}
	remoteType = strings.ToLower(remoteType)
	return endpointInfo{
		Kind:       endpointCloud,
		RemoteName: remoteName,
		RemoteType: remoteType,
		RateRisk:   rateRiskRemoteType(remoteType),
		Known:      true,
	}
}

func configuredRemoteTypes() map[string]string {
	types := make(map[string]string)
	for _, remote := range fsConfig.GetRemotes() {
		types[strings.ToLower(remote.Name)] = strings.ToLower(remote.Type)
	}
	return types
}

func remoteName(path string) (string, bool) {
	colon := strings.Index(path, ":")
	if colon <= 0 {
		return "", false
	}
	if colon == 1 && ((path[0] >= 'A' && path[0] <= 'Z') || (path[0] >= 'a' && path[0] <= 'z')) {
		return "", false
	}
	if slash := strings.IndexAny(path, `/\`); slash != -1 && slash < colon {
		return "", false
	}
	return path[:colon], true
}

func hasUnknownRemote(endpoint endpointInfo) bool {
	return endpoint.Kind == endpointUnknown || (endpoint.Kind == endpointCloud && !endpoint.Known)
}

func rateRiskRemoteType(remoteType string) bool {
	switch remoteType {
	case "drive", "dropbox", "onedrive", "iclouddrive", "googlephotos", "box", "mega", "pcloud", "yandex":
		return true
	default:
		return false
	}
}

func resolverTPSLimit(endpoints ...endpointInfo) float64 {
	limit := 0.0
	for _, endpoint := range endpoints {
		tps := remoteTPSLimit(endpoint.RemoteType)
		if tps == 0 {
			continue
		}
		if limit == 0 || tps < limit {
			limit = tps
		}
	}
	return limit
}

func remoteTPSLimit(remoteType string) float64 {
	switch remoteType {
	case "iclouddrive":
		return 3
	case "drive", "googlephotos":
		return 4
	case "dropbox", "onedrive", "box", "mega", "pcloud", "yandex":
		return 6
	default:
		return 0
	}
}
