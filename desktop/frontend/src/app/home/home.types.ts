/**
 * Type definitions for the home component
 */

import { Action } from '../app.service';
import { models } from '../../../wailsjs/go/models';

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
  color: 'primary' | 'accent' | 'warn';
  icon: string;
  label: string;
}

// Type guards
export function isValidConfigInfo(configInfo: any): configInfo is ConfigInfo {
  return configInfo && 
         typeof configInfo.working_dir === 'string' &&
         typeof configInfo.selected_profile_index === 'number' &&
         Array.isArray(configInfo.profiles);
}

export function isValidProfile(profile: any): profile is Profile {
  return profile && 
         typeof profile.name === 'string' &&
         typeof profile.from === 'string' &&
         typeof profile.to === 'string' &&
         Array.isArray(profile.included_paths) &&
         Array.isArray(profile.excluded_paths) &&
         typeof profile.bandwidth === 'number' &&
         typeof profile.parallel === 'number';
}

export function isValidProfileIndex(configInfo: ConfigInfo, index: number): boolean {
  return index >= 0 && 
         index < configInfo.profiles.length &&
         isValidProfile(configInfo.profiles[index]);
}

// Helper functions
export function getActionConfig(action: Action): ActionConfig {
  switch (action) {
    case Action.Pull:
      return { color: 'primary', icon: 'download', label: 'Pulling' };
    case Action.Push:
      return { color: 'accent', icon: 'upload', label: 'Pushing' };
    case Action.Bi:
      return { color: 'primary', icon: 'sync', label: 'Syncing' };
    case Action.BiResync:
      return { color: 'warn', icon: 'refresh', label: 'Resyncing' };
    default:
      return { color: 'primary', icon: 'play_arrow', label: 'Running' };
  }
}

export function parseProfileSelection(value: any): number | null {
  if (value === null || value === undefined || value === '') {
    return null;
  }
  const parsed = parseInt(value);
  return isNaN(parsed) ? null : parsed;
}

export function validateTabProfileSelection(
  configInfo: ConfigInfo, 
  selectedIndex: number | null
): boolean {
  if (selectedIndex === null) return false;
  return isValidProfileIndex(configInfo, selectedIndex);
}
