/**
 * Wails SyncConfig — stored in Operation.sync_config (JSON).
 * Supports both camelCase (Wails) and snake_case keys on read.
 */
import { normalizeFlowAction, type FlowAction } from '@/constants/forms'

export interface SyncConfig {
  action: FlowAction

  // Performance
  parallel?: number
  bandwidth?: number // MB/s
  multiThreadStreams?: number
  bufferSize?: string
  retries?: number
  lowLevelRetries?: number
  maxDuration?: string
  checkFirst?: boolean
  orderBy?: string
  retriesSleep?: string
  tpsLimit?: number
  connTimeout?: string
  ioTimeout?: string

  // Filtering
  includedPaths?: string[]
  excludedPaths?: string[]
  minSize?: string
  maxSize?: string
  maxAge?: string
  minAge?: string
  maxDepth?: number
  filterFromFile?: string
  excludeIfPresent?: string
  useRegex?: boolean
  deleteExcluded?: boolean

  // Safety
  dryRun?: boolean
  maxDelete?: number
  immutable?: boolean
  maxTransfer?: string
  maxDeleteSize?: string
  suffix?: string
  suffixKeepExtension?: boolean
  backupPath?: string

  // Comparison
  sizeOnly?: boolean
  updateMode?: boolean
  ignoreExisting?: boolean

  // Sync-specific (push)
  deleteTiming?: 'before' | 'during' | 'after' | ''

  // Bisync
  conflictResolution?: string
  resilient?: boolean
  maxLock?: string
  checkAccess?: boolean
  conflictLoser?: string
  conflictSuffix?: string
}

function pickNum(raw: Record<string, unknown>, ...keys: string[]): number | undefined {
  for (const k of keys) {
    const v = raw[k]
    if (typeof v === 'number' && !Number.isNaN(v)) return v
    if (typeof v === 'string' && v.trim() !== '' && !Number.isNaN(Number(v))) return Number(v)
  }
  return undefined
}

function pickStr(raw: Record<string, unknown>, ...keys: string[]): string | undefined {
  for (const k of keys) {
    const v = raw[k]
    if (typeof v === 'string' && v.trim() !== '') return v
  }
  return undefined
}

function pickBool(raw: Record<string, unknown>, ...keys: string[]): boolean | undefined {
  for (const k of keys) {
    const v = raw[k]
    if (typeof v === 'boolean') return v
  }
  return undefined
}

function pickStrArr(raw: Record<string, unknown>, ...keys: string[]): string[] | undefined {
  for (const k of keys) {
    const v = raw[k]
    if (Array.isArray(v)) {
      return v.filter((x): x is string => typeof x === 'string')
    }
  }
  return undefined
}

/** Normalize raw sync_config JSON into a typed SyncConfig. */
export function parseSyncConfig(raw: unknown, fallbackAction: string = 'push'): SyncConfig {
  const o =
    raw && typeof raw === 'object' && !Array.isArray(raw)
      ? (raw as Record<string, unknown>)
      : {}
  const action = normalizeFlowAction(
    (typeof o.action === 'string' && o.action) || fallbackAction,
  )
  const cfg: SyncConfig = { action }

  const bag = cfg as unknown as Record<string, unknown>
  const n = (camel: keyof SyncConfig, ...keys: string[]) => {
    const v = pickNum(o, ...keys)
    if (v !== undefined) bag[camel as string] = v
  }
  const s = (camel: keyof SyncConfig, ...keys: string[]) => {
    const v = pickStr(o, ...keys)
    if (v !== undefined) bag[camel as string] = v
  }
  const b = (camel: keyof SyncConfig, ...keys: string[]) => {
    const v = pickBool(o, ...keys)
    if (v !== undefined) bag[camel as string] = v
  }
  const a = (camel: keyof SyncConfig, ...keys: string[]) => {
    const v = pickStrArr(o, ...keys)
    if (v !== undefined) bag[camel as string] = v
  }

  n('parallel', 'parallel')
  n('bandwidth', 'bandwidth')
  n('multiThreadStreams', 'multiThreadStreams', 'multi_thread_streams')
  s('bufferSize', 'bufferSize', 'buffer_size')
  n('retries', 'retries')
  n('lowLevelRetries', 'lowLevelRetries', 'low_level_retries')
  s('maxDuration', 'maxDuration', 'max_duration')
  b('checkFirst', 'checkFirst', 'check_first')
  s('orderBy', 'orderBy', 'order_by')
  s('retriesSleep', 'retriesSleep', 'retries_sleep')
  n('tpsLimit', 'tpsLimit', 'tps_limit')
  s('connTimeout', 'connTimeout', 'conn_timeout')
  s('ioTimeout', 'ioTimeout', 'io_timeout')

  a('includedPaths', 'includedPaths', 'included_paths')
  a('excludedPaths', 'excludedPaths', 'excluded_paths')
  s('minSize', 'minSize', 'min_size')
  s('maxSize', 'maxSize', 'max_size')
  s('maxAge', 'maxAge', 'max_age')
  s('minAge', 'minAge', 'min_age')
  n('maxDepth', 'maxDepth', 'max_depth')
  s('filterFromFile', 'filterFromFile', 'filter_from_file')
  s('excludeIfPresent', 'excludeIfPresent', 'exclude_if_present')
  b('useRegex', 'useRegex', 'use_regex')
  b('deleteExcluded', 'deleteExcluded', 'delete_excluded')

  b('dryRun', 'dryRun', 'dry_run')
  n('maxDelete', 'maxDelete', 'max_delete')
  b('immutable', 'immutable')
  s('maxTransfer', 'maxTransfer', 'max_transfer')
  s('maxDeleteSize', 'maxDeleteSize', 'max_delete_size')
  s('suffix', 'suffix')
  b('suffixKeepExtension', 'suffixKeepExtension', 'suffix_keep_extension')
  s('backupPath', 'backupPath', 'backup_path')

  b('sizeOnly', 'sizeOnly', 'size_only')
  b('updateMode', 'updateMode', 'update_mode')
  b('ignoreExisting', 'ignoreExisting', 'ignore_existing')

  s('deleteTiming', 'deleteTiming', 'delete_timing')

  s('conflictResolution', 'conflictResolution', 'conflict_resolution')
  b('resilient', 'resilient')
  s('maxLock', 'maxLock', 'max_lock')
  b('checkAccess', 'checkAccess', 'check_access')
  s('conflictLoser', 'conflictLoser', 'conflict_loser')
  s('conflictSuffix', 'conflictSuffix', 'conflict_suffix')

  return cfg
}

/** Persist shape: prefer snake_case for Go store/profile alignment + camel for Wails. */
export function serializeSyncConfig(cfg: SyncConfig): Record<string, unknown> {
  const out: Record<string, unknown> = { action: cfg.action }
  const set = (snake: string, camel: string, v: unknown) => {
    if (v === undefined || v === null || v === '') return
    if (Array.isArray(v) && v.length === 0) return
    out[snake] = v
    out[camel] = v
  }
  set('parallel', 'parallel', cfg.parallel)
  set('bandwidth', 'bandwidth', cfg.bandwidth)
  set('multi_thread_streams', 'multiThreadStreams', cfg.multiThreadStreams)
  set('buffer_size', 'bufferSize', cfg.bufferSize)
  set('retries', 'retries', cfg.retries)
  set('low_level_retries', 'lowLevelRetries', cfg.lowLevelRetries)
  set('max_duration', 'maxDuration', cfg.maxDuration)
  set('check_first', 'checkFirst', cfg.checkFirst)
  set('order_by', 'orderBy', cfg.orderBy)
  set('retries_sleep', 'retriesSleep', cfg.retriesSleep)
  set('tps_limit', 'tpsLimit', cfg.tpsLimit)
  set('conn_timeout', 'connTimeout', cfg.connTimeout)
  set('io_timeout', 'ioTimeout', cfg.ioTimeout)
  set('included_paths', 'includedPaths', cfg.includedPaths)
  set('excluded_paths', 'excludedPaths', cfg.excludedPaths)
  set('min_size', 'minSize', cfg.minSize)
  set('max_size', 'maxSize', cfg.maxSize)
  set('max_age', 'maxAge', cfg.maxAge)
  set('min_age', 'minAge', cfg.minAge)
  set('max_depth', 'maxDepth', cfg.maxDepth)
  set('filter_from_file', 'filterFromFile', cfg.filterFromFile)
  set('exclude_if_present', 'excludeIfPresent', cfg.excludeIfPresent)
  set('use_regex', 'useRegex', cfg.useRegex)
  set('delete_excluded', 'deleteExcluded', cfg.deleteExcluded)
  set('dry_run', 'dryRun', cfg.dryRun)
  set('max_delete', 'maxDelete', cfg.maxDelete)
  set('immutable', 'immutable', cfg.immutable)
  set('max_transfer', 'maxTransfer', cfg.maxTransfer)
  set('max_delete_size', 'maxDeleteSize', cfg.maxDeleteSize)
  set('suffix', 'suffix', cfg.suffix)
  set('suffix_keep_extension', 'suffixKeepExtension', cfg.suffixKeepExtension)
  set('backup_path', 'backupPath', cfg.backupPath)
  set('size_only', 'sizeOnly', cfg.sizeOnly)
  set('update_mode', 'updateMode', cfg.updateMode)
  set('ignore_existing', 'ignoreExisting', cfg.ignoreExisting)
  set('delete_timing', 'deleteTiming', cfg.deleteTiming)
  set('conflict_resolution', 'conflictResolution', cfg.conflictResolution)
  set('resilient', 'resilient', cfg.resilient)
  set('max_lock', 'maxLock', cfg.maxLock)
  set('check_access', 'checkAccess', cfg.checkAccess)
  set('conflict_loser', 'conflictLoser', cfg.conflictLoser)
  set('conflict_suffix', 'conflictSuffix', cfg.conflictSuffix)
  return out
}

export function defaultSyncConfig(action: FlowAction = 'push'): SyncConfig {
  return { action }
}

/** Short chips for view mode when non-default options are set. */
export function syncConfigSummaryChips(cfg: SyncConfig): string[] {
  const chips: string[] = []
  if (cfg.dryRun) chips.push('dry-run')
  if (cfg.parallel && cfg.parallel > 0) chips.push(`×${cfg.parallel}`)
  if (cfg.bandwidth && cfg.bandwidth > 0) chips.push(`${cfg.bandwidth}M`)
  if (cfg.includedPaths?.length) chips.push(`+${cfg.includedPaths.length} include`)
  if (cfg.excludedPaths?.length) chips.push(`−${cfg.excludedPaths.length} exclude`)
  if (cfg.maxAge) chips.push(`max-age ${cfg.maxAge}`)
  if (cfg.minSize || cfg.maxSize) chips.push('size filter')
  if (cfg.conflictResolution) chips.push(`conflict:${cfg.conflictResolution}`)
  if (cfg.immutable) chips.push('immutable')
  if (cfg.sizeOnly) chips.push('size-only')
  return chips
}
