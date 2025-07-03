/**
 * Type definitions for the home component
 */

import { Action } from "../app.service";
import { models } from "../../../wailsjs/go/models";

// Re-export types for convenience
export type Profile = models.Profile;
export type ConfigInfo = models.ConfigInfo;

// Interface for tab update data
export interface TabUpdateData {
  selectedProfileIndex?: number | null;
  isStopping?: boolean;
  data?: string[];
}

// Interface for action configuration
export interface ActionConfig {
  color: "primary" | "accent" | "warn";
  icon: string;
  label: string;
}

// Type guards
export function isValidConfigInfo(
  configInfo: unknown
): configInfo is ConfigInfo {
  return (
    configInfo !== null &&
    typeof configInfo === "object" &&
    configInfo !== undefined &&
    "working_dir" in configInfo &&
    "selected_profile_index" in configInfo &&
    "profiles" in configInfo &&
    typeof (configInfo as ConfigInfo).working_dir === "string" &&
    typeof (configInfo as ConfigInfo).selected_profile_index === "number" &&
    Array.isArray((configInfo as ConfigInfo).profiles)
  );
}

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
    typeof (profile as Profile).name === "string" &&
    typeof (profile as Profile).from === "string" &&
    typeof (profile as Profile).to === "string" &&
    Array.isArray((profile as Profile).included_paths) &&
    Array.isArray((profile as Profile).excluded_paths) &&
    typeof (profile as Profile).bandwidth === "number" &&
    typeof (profile as Profile).parallel === "number"
  );
}

export function isValidProfileIndex(
  configInfo: ConfigInfo,
  index: number
): boolean {
  return (
    index >= 0 &&
    index < configInfo.profiles.length &&
    isValidProfile(configInfo.profiles[index])
  );
}

// Helper functions
export function getActionConfig(action: Action): ActionConfig {
  switch (action) {
    case Action.Pull:
      return { color: "primary", icon: "download", label: "Pulling" };
    case Action.Push:
      return { color: "accent", icon: "upload", label: "Pushing" };
    case Action.Bi:
      return { color: "primary", icon: "sync", label: "Syncing" };
    case Action.BiResync:
      return { color: "warn", icon: "refresh", label: "Resyncing" };
    default:
      return { color: "primary", icon: "play_arrow", label: "Running" };
  }
}

export function parseProfileSelection(value: unknown): number | null {
  if (value === null || value === undefined || value === "") {
    return null;
  }
  const parsed = parseInt(String(value));
  return isNaN(parsed) ? null : parsed;
}

export function validateTabProfileSelection(
  configInfo: ConfigInfo,
  selectedIndex: number | null
): boolean {
  if (selectedIndex === null) return false;
  return isValidProfileIndex(configInfo, selectedIndex);
}
