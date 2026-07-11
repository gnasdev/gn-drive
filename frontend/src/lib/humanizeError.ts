/**
 * Turn backend/rclone error blobs into short user-facing messages.
 * Raw stderr (JSON logs, "signal: killed") is not useful in the status panel.
 */
export function humanizeError(raw?: string | null, status?: string): string {
  if (status === 'cancelled' || status === 'cancelling') {
    return '' // UI uses a dedicated cancelled label, not an error box
  }
  const s = (raw ?? '').trim()
  if (!s) return ''

  const lower = s.toLowerCase()
  if (
    lower.includes('signal: killed') ||
    lower.includes('signal: interrupt') ||
    lower.includes('context canceled') ||
    lower.includes('context cancelled') ||
    lower.includes('errtaskcancelled') ||
    lower.includes('task cancelled')
  ) {
    return '' // cancelled — not an error
  }

  // Strip rclone wrapper noise.
  let msg = s
  if (msg.startsWith('rclone:')) {
    msg = msg.replace(/^rclone:\s*/i, '')
  }
  // Drop giant JSON stderr dumps in parentheses.
  const paren = msg.indexOf('(stderr:')
  if (paren >= 0) {
    msg = msg.slice(0, paren).trim()
  }
  // If still looks like JSON, summarize.
  if (msg.startsWith('{') || msg.includes('"stats"')) {
    return 'Sync failed. Check paths and remote configuration.'
  }
  // Cap length for UI.
  if (msg.length > 160) {
    msg = msg.slice(0, 157) + '…'
  }
  return msg
}

/** True when the run was stopped by the user (not a hard failure). */
export function isUserCancelError(raw?: string | null, status?: string): boolean {
  if (status === 'cancelled' || status === 'cancelling') return true
  const s = (raw ?? '').toLowerCase()
  return (
    s.includes('signal: killed') ||
    s.includes('signal: interrupt') ||
    s.includes('context canceled') ||
    s.includes('task cancelled')
  )
}
