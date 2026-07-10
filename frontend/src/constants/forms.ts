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

/** Sync / profile direction actions. */
export const SYNC_ACTIONS = ['pull', 'push', 'bi', 'bi-resync'] as const

export type SyncAction = (typeof SYNC_ACTIONS)[number]

/** Board edge actions (includes one-shot copy). */
export const BOARD_EDGE_ACTIONS = ['push', 'pull', 'copy', 'bi'] as const

export type BoardEdgeAction = (typeof BOARD_EDGE_ACTIONS)[number]

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

/** Split composed path into board node remote_name + path. */
export function composedPathToBoardNode(composed: string): { remote_name: string; path: string } {
  const p = parseRemotePath(composed)
  if (p.mode === 'local') {
    return { remote_name: '', path: p.path.trim() || '/' }
  }
  const path = p.path.trim()
  return {
    remote_name: p.remote.trim(),
    path: path ? (path.startsWith('/') ? path : `/${path}`) : '/',
  }
}

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
