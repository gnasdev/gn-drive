/**
 * Profile form validator — port of desktop ProfileValidator rules for fields
 * exposed in the web UI (name, from, to, parallel, bandwidth, direction).
 */

import { isAbsoluteLocalPath, parseRemotePath, PROFILE_DIRECTIONS } from '@/constants/forms'

export type ProfileField =
  | 'name'
  | 'from'
  | 'to'
  | 'parallel'
  | 'bandwidth'
  | 'direction'

export interface ProfileValidationError {
  field: ProfileField
  /** i18n key under profiles.validation.* */
  messageKey: string
  /** optional params for i18n */
  params?: Record<string, string | number>
}

export interface ProfileDraftLike {
  name?: string
  from?: string
  to?: string
  parallel?: number | null
  bandwidth?: number | null
  direction?: string
}

const REMOTE_NAME_RE = /^[a-zA-Z0-9_-]+$/

export function validateProfileName(name: string): ProfileValidationError | null {
  const n = (name ?? '').trim()
  if (!n) return { field: 'name', messageKey: 'nameEmpty' }
  if (n.length > 100) return { field: 'name', messageKey: 'nameTooLong' }
  if (n.includes('..') || n.includes('/') || n.includes('\\')) {
    return { field: 'name', messageKey: 'nameInvalidChars' }
  }
  return null
}

export function validateRclonePath(path: string, field: 'from' | 'to'): ProfileValidationError | null {
  const p = (path ?? '').trim()
  if (!p) return { field, messageKey: 'pathEmpty' }
  if (p.includes('..')) return { field, messageKey: 'pathTraversal' }
  if (p.includes('\0')) return { field, messageKey: 'pathInvalidChars' }

  // Absolute local (Unix) or Windows drive letter path
  if (isAbsoluteLocalPath(p) || /^[a-zA-Z]:[\\/]/.test(p)) {
    if (p === '/' || /^[a-zA-Z]:[\\/]?$/.test(p)) {
      return { field, messageKey: 'pathTooShort' }
    }
    return null
  }

  // remote:path
  const parsed = parseRemotePath(p)
  if (parsed.mode !== 'remote' || !parsed.remote) {
    return { field, messageKey: 'pathRemoteFormat' }
  }
  if (!REMOTE_NAME_RE.test(parsed.remote)) {
    return { field, messageKey: 'pathRemoteNameChars' }
  }
  if (parsed.remote.length > 50) {
    return { field, messageKey: 'pathRemoteNameLong' }
  }
  return null
}

export function validateParallel(parallel: number | null | undefined): ProfileValidationError | null {
  const n = parallel ?? 0
  if (typeof n !== 'number' || Number.isNaN(n)) {
    return { field: 'parallel', messageKey: 'parallelInvalid' }
  }
  if (n < 0) return { field: 'parallel', messageKey: 'parallelNegative' }
  if (n > 256) return { field: 'parallel', messageKey: 'parallelTooHigh' }
  return null
}

export function validateBandwidth(bandwidth: number | null | undefined): ProfileValidationError | null {
  const n = bandwidth ?? 0
  if (typeof n !== 'number' || Number.isNaN(n)) {
    return { field: 'bandwidth', messageKey: 'bandwidthInvalid' }
  }
  if (n < 0) return { field: 'bandwidth', messageKey: 'bandwidthNegative' }
  if (n > 10000) return { field: 'bandwidth', messageKey: 'bandwidthTooHigh' }
  return null
}

export function validateDirection(direction: string | undefined): ProfileValidationError | null {
  const d = (direction ?? '').trim()
  if (!d) return { field: 'direction', messageKey: 'directionEmpty' }
  // Profiles only allow push / bi / bi-resync (not pull or other sync actions).
  if (!(PROFILE_DIRECTIONS as readonly string[]).includes(d)) {
    return { field: 'direction', messageKey: 'directionInvalid' }
  }
  return null
}

/** Full profile draft validation — returns all errors (not just first). */
export function validateProfileDraft(draft: ProfileDraftLike): ProfileValidationError[] {
  const errors: ProfileValidationError[] = []
  const push = (e: ProfileValidationError | null) => {
    if (e) errors.push(e)
  }
  push(validateProfileName(draft.name ?? ''))
  push(validateDirection(draft.direction))
  push(validateRclonePath(draft.from ?? '', 'from'))
  push(validateRclonePath(draft.to ?? '', 'to'))
  push(validateParallel(draft.parallel))
  push(validateBandwidth(draft.bandwidth))
  return errors
}

export function isProfileDraftValid(draft: ProfileDraftLike): boolean {
  return validateProfileDraft(draft).length === 0
}

export function errorsByField(
  errors: ProfileValidationError[],
): Partial<Record<ProfileField, ProfileValidationError>> {
  const out: Partial<Record<ProfileField, ProfileValidationError>> = {}
  for (const e of errors) {
    if (!out[e.field]) out[e.field] = e
  }
  return out
}
