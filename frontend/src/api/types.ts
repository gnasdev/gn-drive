// TypeScript types mirroring the Go API JSON contracts.
// These match the structs in internal/store/models.go and the DTOs returned
// by internal/api/handlers.

export interface Profile {
  name: string
  from: string
  to: string
  included_paths?: string[] | null
  excluded_paths?: string[] | null
  bandwidth: number
  parallel: number
  backup_path?: string
  cache_path?: string
  min_size?: string
  max_size?: string
  filter_from_file?: string
  exclude_if_present?: string
  use_regex?: boolean
  max_age?: string
  min_age?: string
  max_depth?: number | null
  delete_excluded?: boolean
  max_delete?: number | null
  immutable?: boolean
  conflict_resolution?: string
  dry_run?: boolean
  max_transfer?: string
  max_delete_size?: string
  suffix?: string
  suffix_keep_extension?: boolean
  multi_thread_streams?: number | null
  buffer_size?: string
  retries?: number | null
  low_level_retries?: number | null
  max_duration?: string
  check_first?: boolean
  order_by?: string
  retries_sleep?: string
  tps_limit?: number | null
  conn_timeout?: string
  io_timeout?: string
  size_only?: boolean
  update_mode?: boolean
  ignore_existing?: boolean
  delete_timing?: string
  resilient?: boolean
  max_lock?: string
  check_access?: boolean
  conflict_loser?: string
  conflict_suffix?: string
  fast_list?: boolean
}

export interface Schedule {
  id: string
  profile_name: string
  action: string
  cron: string
  enabled: boolean
  last_run?: string
  next_run?: string
  last_result?: string
  created_at?: string
}

export interface Remote {
  name: string
  type: string
  description?: string
}

export interface Board {
  id: string
  name: string
  description: string
  created_at: string
  updated_at: string
  nodes?: BoardNode[]
  edges?: BoardEdge[]
}

export interface BoardNode {
  id: string
  remote_name: string
  path: string
  label: string
  x: number
  y: number
}

export interface BoardEdge {
  id: string
  source_id: string
  target_id: string
  action: string
  sync_config?: any
}

export interface Flow {
  id: string
  name: string
  schedule_cron?: string
  enabled: boolean
  created_at?: string
  updated_at?: string
  operations?: any[]
}

export interface HistoryEntry {
  id: string
  profile_name: string
  action: string
  state: string
  started_at: string
  finished_at?: string
  duration_secs: number
  bytes: number
  errors: number
  files: number
  error_message?: string
}

export interface HistoryStats {
  total_syncs: number
  total_bytes: number
  total_duration_secs: number
  total_errors: number
  by_profile: Record<string, ProfileStats>
}

export interface ProfileStats {
  syncs: number
  bytes: number
  duration_secs: number
  errors: number
}

export interface SyncTask {
  id: string
  name: string
  action: string
  status: string
  started_at?: string
  ended_at?: string
  transferred?: number
  total?: number
  bytes_per_sec?: number
  errors?: number
  files_transferred?: number
  total_files?: number
}

export interface AppStatus {
  setup: boolean
  unlocked: boolean
  version: string
  lockout?: {
    failed_attempts: number
    locked_until: string
    is_locked: boolean
    retry_after_secs: number
  }
}

export interface ServiceStatus {
  platform: string
  scope: string
  installed: boolean
  running: boolean
  pid: number
  web_port: number
  uptime_secs: number
  started_at: string
  last_heartbeat: string
  last_error: string
  active_tasks: string[]
  heartbeat_stale?: boolean
}

export interface FileEntry {
  name: string
  size: number
  mime_type: string
  is_dir: boolean
  mod_time: string
  path: string
  id: string
}

export interface AppSettings {
  theme?: string
  notifications_enabled?: string
  debug_mode?: string
  minimize_to_tray?: string
  start_at_login?: string
  [k: string]: string | undefined
}
