/**
 * Type definitions for the profiles component
 */

import * as models from "../../../wailsjs/desktop/backend/models/models.js";

// Re-export the Profile type for convenience
export type Profile = models.Profile;

// Path type for include/exclude paths
export type PathType = "file" | "folder";

// Interface for path configuration
export interface PathConfig {
  type: PathType;
  value: string;
}

// Interface for remote path configuration
export interface RemotePathConfig {
  remote: string;
  path: string;
}

// Type guards
export function isValidProfile(profile: unknown): profile is Profile {
  return (
    profile !== null &&
    typeof profile === "object" &&
    profile !== undefined &&
    "name" in profile &&
    "from" in profile &&
    "to" in profile &&
    "included_paths" in profile &&
    "excluded_paths" in profile &&
    "bandwidth" in profile &&
    "parallel" in profile &&
    "backup_path" in profile &&
    "cache_path" in profile &&
    typeof (profile as Profile).name === "string" &&
    typeof (profile as Profile).from === "string" &&
    typeof (profile as Profile).to === "string" &&
    Array.isArray((profile as Profile).included_paths) &&
    Array.isArray((profile as Profile).excluded_paths) &&
    typeof (profile as Profile).bandwidth === "number" &&
    typeof (profile as Profile).parallel === "number" &&
    typeof (profile as Profile).backup_path === "string" &&
    typeof (profile as Profile).cache_path === "string"
  );
}

export function isValidProfileIndex(
  profiles: Profile[],
  index: number
): boolean {
  return index >= 0 && index < profiles.length;
}

export function isValidPathIndex(paths: string[], index: number): boolean {
  return index >= 0 && index < paths.length;
}

// Helper functions for path parsing
export function parseRemotePath(path: string): RemotePathConfig {
  const colonIndex = path.indexOf(":");
  if (colonIndex > 0) {
    return {
      remote: path.substring(0, colonIndex),
      path: path.substring(colonIndex + 1),
    };
  }
  return {
    remote: "",
    path: path,
  };
}

export function buildRemotePath(remote: string, path: string): string {
  if (remote) {
    return `${remote}:${path || ""}`;
  }
  return path || "";
}

export function parsePathConfig(path: string): PathConfig {
  if (path.endsWith("/**")) {
    return {
      type: "folder",
      value: path.slice(0, -3),
    };
  }
  return {
    type: "file",
    value: path,
  };
}

export function buildPath(config: PathConfig): string {
  if (config.type === "folder") {
    return config.value + "/**";
  }
  return config.value;
}

// Constants
export const DEFAULT_BANDWIDTH_OPTIONS = [1, 2, 5, 10, 20, 50, 100];
export const DEFAULT_PARALLEL_OPTIONS = [1, 2, 4, 8, 16, 32, 64];
