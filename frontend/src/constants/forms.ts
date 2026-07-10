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

export type OpRisk = 'safe' | 'caution' | 'danger'

/** Risk level only; copy lives in i18n (syncHelp.*). */
export const SYNC_ACTION_META: Record<SyncAction, { risk: OpRisk }> = {
  pull: { risk: 'caution' },
  push: { risk: 'caution' },
  bi: { risk: 'caution' },
  'bi-resync': { risk: 'danger' },
}

/** One-shot file operations available on Operations page. */
export const FILE_OPS = ['copy', 'move', 'check', 'mkdir', 'purge', 'delete'] as const

export type FileOpKind = (typeof FILE_OPS)[number]

export const FILE_OP_META: Record<
  FileOpKind,
  { fields: 'source-dest' | 'path'; risk: OpRisk }
> = {
  copy: { fields: 'source-dest', risk: 'safe' },
  move: { fields: 'source-dest', risk: 'caution' },
  check: { fields: 'source-dest', risk: 'safe' },
  mkdir: { fields: 'path', risk: 'safe' },
  purge: { fields: 'path', risk: 'danger' },
  delete: { fields: 'path', risk: 'danger' },
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
