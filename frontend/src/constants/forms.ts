/** Curated rclone remote types for form selects (phase 1). */
export const REMOTE_TYPES = [
  'local',
  'drive',
  's3',
  'sftp',
  'dropbox',
  'onedrive',
  'webdav',
  'ftp',
  'crypt',
  'alias',
  'b2',
  'mega',
  'box',
  'pcloud',
] as const

export type RemoteType = (typeof REMOTE_TYPES)[number]

/**
 * Profile / Operations one-shot actions (includes pull).
 */
export const SYNC_ACTIONS = ['pull', 'push', 'bi', 'bi-resync'] as const

export type SyncAction = (typeof SYNC_ACTIONS)[number]

/**
 * Flow operation actions only:
 * 1-way (push), 2-way (bi), 2-way resync (bi-resync).
 * Pull is not offered on flow operations.
 */
export const FLOW_ACTIONS = ['push', 'bi', 'bi-resync'] as const

export type FlowAction = (typeof FLOW_ACTIONS)[number]

export function isFlowAction(v: string | undefined | null): v is FlowAction {
  return !!v && (FLOW_ACTIONS as readonly string[]).includes(v)
}

/** Coerce legacy/invalid flow action (e.g. pull) to push. */
export function normalizeFlowAction(v: string | undefined | null): FlowAction {
  if (isFlowAction(v)) return v
  return 'push'
}

/**
 * Profile direction options:
 * 1-way (push), 2-way (bi), 2-way resync (bi-resync).
 */
export const PROFILE_DIRECTIONS = ['push', 'bi', 'bi-resync'] as const

export type ProfileDirection = (typeof PROFILE_DIRECTIONS)[number]

export function isProfileDirection(v: string | undefined | null): v is ProfileDirection {
  return !!v && (PROFILE_DIRECTIONS as readonly string[]).includes(v)
}

/** Coerce legacy/invalid profile direction to a valid default. */
export function normalizeProfileDirection(v: string | undefined | null): ProfileDirection {
  if (isProfileDirection(v)) return v
  return 'push'
}

/** Common 5-field cron presets for flow schedule_cron. */
export const CRON_PRESETS = [
  { value: '0 * * * *', key: 'everyHour' },
  { value: '0 */6 * * *', key: 'every6Hours' },
  { value: '0 0 * * *', key: 'dailyMidnight' },
  { value: '0 9 * * 1-5', key: 'weekdaysMorning' },
  { value: '0 0 * * 0', key: 'weeklySunday' },
] as const

export type CronPresetValue = (typeof CRON_PRESETS)[number]['value']

export const HISTORY_STATES = ['running', 'completed', 'failed', 'cancelled'] as const

export function isAbsoluteLocalPath(path: string): boolean {
  return path.startsWith('/')
}

/** Parse "remote:path" or absolute local path into mode parts. */
export function parseRemotePath(value: string): {
  mode: 'local' | 'remote'
  remote: string
  path: string
} {
  const v = (value ?? '').trim()
  if (!v) {
    return { mode: 'local', remote: '', path: '' }
  }
  if (isAbsoluteLocalPath(v)) {
    return { mode: 'local', remote: '', path: v }
  }
  const colon = v.indexOf(':')
  if (colon > 0) {
    return {
      mode: 'remote',
      remote: v.slice(0, colon),
      path: v.slice(colon + 1).replace(/^\/+/, '') || '',
    }
  }
  return { mode: 'remote', remote: v, path: '' }
}

/** Compose full path string for profile from/to storage. */
export function composeRemotePath(
  mode: 'local' | 'remote',
  remote: string,
  path: string,
): string {
  if (mode === 'local') {
    return path.trim()
  }
  const name = remote.trim()
  if (!name) return ''
  let p = path.trim()
  if (p.startsWith('/')) p = p.slice(1)
  if (!p) return `${name}:`
  return `${name}:/${p}`
}

/** Browse root for rclone lsjson. */
export function browseRoot(mode: 'local' | 'remote', remote: string, path: string): string {
  if (mode === 'local') {
    const p = path.trim() || '/'
    return p
  }
  const name = remote.trim()
  if (!name) return ''
  let p = path.trim()
  if (p.startsWith('/')) p = p.slice(1)
  if (!p) return `${name}:`
  return `${name}:/${p}`
}
