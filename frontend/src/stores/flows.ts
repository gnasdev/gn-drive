import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { useApi } from '@/composables/useApi'
import type { FileTransferInfo, Flow, FlowOpSyncStatus, Operation } from '@/api/types'
import { normalizeFlowAction } from '@/constants/forms'

function newId(): string {
  return crypto.randomUUID()
}

/** Wails createEmptyOperation: action lives in sync_config and top-level column. */
export function emptyOperation(): Operation {
  return {
    id: newId(),
    source_remote: '',
    source_path: '/',
    target_remote: '',
    target_path: '/',
    action: 'push',
    sync_config: { action: 'push' },
    is_expanded: true,
    sort_order: 0,
    status: 'idle',
  }
}

/**
 * Effective flow action: column first, then sync_config.action.
 * Always normalized to push | bi | bi-resync (pull not allowed on flows).
 */
export function resolveOpAction(op: Operation): string {
  const a = (op.action || '').trim()
  if (a) return normalizeFlowAction(a)
  const sc = op.sync_config as Record<string, unknown> | null | undefined
  if (sc && typeof sc.action === 'string' && sc.action.trim()) {
    return normalizeFlowAction(sc.action.trim())
  }
  return 'push'
}

/** Keep action column and sync_config.action aligned before persist. */
export function withSyncedAction(op: Operation, action?: string): Operation {
  const a = normalizeFlowAction(action ?? resolveOpAction(op))
  const prev = (op.sync_config && typeof op.sync_config === 'object' ? op.sync_config : {}) as Record<
    string,
    unknown
  >
  return {
    ...op,
    action: a,
    sync_config: { ...prev, action: a },
  }
}

export function emptyFlow(): Flow {
  return {
    id: newId(),
    name: '',
    is_collapsed: false,
    schedule_enabled: false,
    enabled: false,
    operations: [],
    status: 'idle',
  }
}

export interface FlowRunLogEntry {
  at: number
  status: string
  opId?: string
  error?: string
  /** Short human label, e.g. "Flow" or "Op #2". */
  label?: string
}

function parseTransfers(
  raw: unknown,
  prev?: FileTransferInfo[],
): FileTransferInfo[] | undefined {
  if (!Array.isArray(raw)) return prev
  const out: FileTransferInfo[] = []
  for (const item of raw) {
    if (!item || typeof item !== 'object') continue
    const o = item as Record<string, unknown>
    const name = String(o.name ?? '')
    if (!name) continue
    out.push({
      name,
      size: Number(o.size ?? 0),
      bytes: Number(o.bytes ?? 0),
      progress: Number(o.progress ?? 0),
      status: String(o.status ?? 'completed'),
      speed: o.speed != null ? Number(o.speed) : undefined,
      error: o.error ? String(o.error) : undefined,
    })
  }
  return out
}

/** Cheap equality for progress snapshots — avoid Pinia churn every SSE frame. */
function syncSnapEqual(a: FlowOpSyncStatus, b: FlowOpSyncStatus): boolean {
  if (a.status !== b.status || a.op_id !== b.op_id) return false
  if (Math.round(a.progress) !== Math.round(b.progress)) return false
  if (a.files_transferred !== b.files_transferred) return false
  if (a.bytes_transferred !== b.bytes_transferred) return false
  if (a.checks !== b.checks || a.errors !== b.errors) return false
  if (a.speed_bps !== b.speed_bps) return false
  if (a.current_file !== b.current_file) return false
  const at = a.transfers ?? []
  const bt = b.transfers ?? []
  if (at.length !== bt.length) return false
  for (let i = 0; i < at.length; i++) {
    if (
      at[i].name !== bt[i].name ||
      at[i].status !== bt[i].status ||
      Math.round(at[i].progress) !== Math.round(bt[i].progress)
    ) {
      return false
    }
  }
  return true
}

export const useFlowsStore = defineStore('flows', () => {
  const api = useApi()
  const items = ref<Flow[]>([])
  /** Runtime status from execute/SSE keyed by flow id. */
  const runStatus = ref<Record<string, string>>({})
  /** Timeline of the latest run per flow (for status panel). */
  const runLog = ref<Record<string, FlowRunLogEntry[]>>({})
  const lastError = ref<Record<string, string>>({})
  /**
   * Live rclone stats for the active op of each flow (Wails SyncStatus).
   * Keyed by flow id. busyKey on backend is `${flowId}:${opId}`.
   */
  const opSyncStatus = ref<Record<string, FlowOpSyncStatus>>({})
  let pollTimer: ReturnType<typeof setInterval> | null = null

  const runningFlowIds = computed(() => {
    const s = new Set<string>()
    for (const [id, st] of Object.entries(runStatus.value)) {
      if (st === 'running' || st === 'cancelling') s.add(id)
    }
    for (const f of items.value) {
      if (f.status === 'running' || f.status === 'cancelling') s.add(f.id)
    }
    return s
  })

  function isFlowRunning(id: string): boolean {
    return runningFlowIds.value.has(id)
  }

  function flowStatusOf(id: string): string {
    return runStatus.value[id] || items.value.find((f) => f.id === id)?.status || 'idle'
  }

  function logOf(id: string): FlowRunLogEntry[] {
    return runLog.value[id] ?? []
  }

  function setRunStatus(id: string, status: string) {
    // Replace object so Pinia/Vue always notify dependents.
    runStatus.value = { ...runStatus.value, [id]: status }
  }

  function appendLog(id: string, entry: FlowRunLogEntry) {
    const prev = runLog.value[id] ?? []
    // Cap length so the panel stays readable.
    const next = [...prev, entry].slice(-40)
    runLog.value = { ...runLog.value, [id]: next }
  }

  function clearRunUi(id: string) {
    runLog.value = { ...runLog.value, [id]: [] }
    const { [id]: _drop, ...rest } = lastError.value
    lastError.value = rest
    const { [id]: _s, ...syncRest } = opSyncStatus.value
    opSyncStatus.value = syncRest
  }

  function activeSyncOf(flowId: string): FlowOpSyncStatus | null {
    return opSyncStatus.value[flowId] ?? null
  }

  /** Map syncengine SSE (profile_id = flowId:opId) onto Wails-style status. */
  function applySyncProgress(topic: string, data: Record<string, unknown>) {
    const profile = String(data.profile_id ?? '')
    const colon = profile.indexOf(':')
    if (colon <= 0) return // not a flow path-sync (e.g. profile name)
    const flowId = profile.slice(0, colon)
    const opId = profile.slice(colon + 1)
    if (!flowId || !opId) return

    const knownFlow = items.value.some((f) => f.id === flowId) || !!runStatus.value[flowId]
    if (!knownFlow && topic === 'sync:started') {
      // still accept if we're about to track it
    }

    const prev = opSyncStatus.value[flowId]
    const transferred = Number(data.transferred ?? data.bytes ?? prev?.bytes_transferred ?? 0)
    const total = Number(data.total ?? data.bytes_total ?? prev?.total_bytes ?? 0)
    const filesXfer = Number(data.files_transferred ?? prev?.files_transferred ?? 0)
    const filesTotal = Number(data.total_files ?? prev?.total_files ?? 0)
    let progress = 0
    if (total > 0) progress = Math.min(100, (transferred / total) * 100)
    else if (filesTotal > 0) progress = Math.min(100, (filesXfer / filesTotal) * 100)
    else if (topic === 'sync:completed') progress = 100
    else if (prev) progress = prev.progress

    let status: FlowOpSyncStatus['status'] = 'running'
    if (topic === 'sync:completed') {
      status = 'completed'
    } else if (topic === 'sync:failed') {
      // Backend may publish cancelled under the failed topic with state=cancelled.
      const st = String(data.state ?? 'failed')
      status = st === 'cancelled' ? 'cancelled' : 'failed'
    } else if (String(data.state ?? '') === 'cancelled') {
      status = 'cancelled'
    } else if (String(data.state ?? '') === 'failed') {
      status = 'failed'
    } else if (String(data.state ?? '') === 'completed') {
      // Final progress frame before sync:completed carries full transfer list.
      status = 'completed'
    }

    const transfers = parseTransfers(data.transfers, prev?.transfers)

    const snap: FlowOpSyncStatus = {
      flow_id: flowId,
      op_id: opId,
      task_id: data.task_id ? String(data.task_id) : prev?.task_id,
      action: String(data.action ?? prev?.action ?? 'push'),
      status,
      progress,
      speed_bps: Number(data.bytes_per_sec ?? prev?.speed_bps ?? 0),
      eta_secs: Number(data.eta_secs ?? prev?.eta_secs ?? 0),
      files_transferred: Number(data.files_transferred ?? prev?.files_transferred ?? 0),
      total_files: Number(data.total_files ?? prev?.total_files ?? 0),
      bytes_transferred: transferred,
      total_bytes: total,
      current_file: String(data.current_file ?? prev?.current_file ?? ''),
      errors: Number(data.errors ?? prev?.errors ?? 0),
      checks: Number(data.checks ?? prev?.checks ?? 0),
      total_checks: Number(data.total_checks ?? prev?.total_checks ?? 0),
      deletes: Number(data.deletes ?? prev?.deletes ?? 0),
      renames: Number(data.renames ?? prev?.renames ?? 0),
      transfers,
      error_message: data.error_message ? String(data.error_message) : prev?.error_message,
      updated_at: Date.now(),
    }

    // Skip store write when snapshot is effectively unchanged (cuts re-renders).
    if (prev && syncSnapEqual(prev, snap)) {
      return
    }
    opSyncStatus.value = { ...opSyncStatus.value, [flowId]: snap }

    // Only push op/flow lifecycle when status or active op changes —
    // NOT on every progress/bytes tick (that rewrote items[] and forced tab reset).
    const prevOpStatus = items.value
      .find((f) => f.id === flowId)
      ?.operations?.find((o) => o.id === opId)?.status
    const lifecycleChanged =
      !prev ||
      prev.status !== status ||
      prev.op_id !== opId ||
      prevOpStatus !== status

    if (!lifecycleChanged) return

    if (status === 'running') {
      applyRunEvent(flowId, 'running', opId)
    } else if (status === 'failed') {
      applyRunEvent(flowId, 'failed', opId, snap.error_message)
    } else if (status === 'completed') {
      applyRunEvent(flowId, 'completed', opId)
    } else if (status === 'cancelled') {
      applyRunEvent(flowId, 'cancelled', opId)
    }
  }

  function ensurePoll() {
    if (pollTimer != null) return
    pollTimer = setInterval(() => {
      void pollRunning()
    }, 1_200)
  }

  function stopPollIfIdle() {
    if (runningFlowIds.value.size > 0) return
    if (pollTimer != null) {
      clearInterval(pollTimer)
      pollTimer = null
    }
  }

  async function pollRunning() {
    const ids = [...runningFlowIds.value]
    if (!ids.length) {
      stopPollIfIdle()
      return
    }
    for (const id of ids) {
      try {
        const remote = await api.get<Flow>(`/api/v1/flows/${encodeURIComponent(id)}`)
        const st = (remote?.status || 'idle').toLowerCase()
        if (st === 'running' || st === 'cancelling') {
          applyRunEvent(id, st)
          continue
        }
        // Prefer explicit terminal status retained by flowengine.lastStatus.
        if (st === 'completed' || st === 'failed' || st === 'cancelled') {
          applyRunEvent(id, st)
          continue
        }
        // Fallback: engine reported idle while UI still thinks running.
        // Infer from last op sync snapshot — never assume success blindly.
        if (st === 'idle' && isFlowRunning(id)) {
          const snap = opSyncStatus.value[id]
          if (snap?.status === 'failed') {
            applyRunEvent(id, 'failed', undefined, snap.error_message)
          } else if (snap?.status === 'cancelled') {
            applyRunEvent(id, 'cancelled')
          } else {
            applyRunEvent(id, 'completed')
          }
        }
      } catch {
        /* ignore poll errors while locked/offline */
      }
    }
    stopPollIfIdle()
  }

  async function load() {
    // Preserve in-memory runtime op statuses across reloads (server does not store them).
    const prevOpStatus = new Map<string, Map<string, string>>()
    for (const f of items.value) {
      const m = new Map<string, string>()
      for (const op of f.operations ?? []) {
        if (op.status && op.status !== 'idle') m.set(op.id, op.status)
      }
      if (m.size) prevOpStatus.set(f.id, m)
    }

    const list = (await api.get<Flow[]>('/api/v1/flows')) ?? []
    items.value = list.map((f) => {
      const runtime = runStatus.value[f.id]
      const opPrev = prevOpStatus.get(f.id)
      return {
        ...f,
        operations: (f.operations ?? []).map((op) => {
          const synced = withSyncedAction(op)
          return {
            ...synced,
            status: opPrev?.get(op.id) || synced.status || 'idle',
          }
        }),
        schedule_enabled: f.schedule_enabled ?? f.enabled ?? false,
        enabled: f.schedule_enabled ?? f.enabled ?? false,
        schedule_cron: f.schedule_cron || f.cron_expr || '',
        cron_expr: f.cron_expr || f.schedule_cron || '',
        status: runtime || f.status || 'idle',
        last_error: lastError.value[f.id],
      }
    })
  }

  async function save(f: Flow) {
    const body: Flow = {
      ...f,
      schedule_enabled: f.schedule_enabled ?? f.enabled,
      enabled: f.schedule_enabled ?? f.enabled,
      cron_expr: f.cron_expr || f.schedule_cron,
      schedule_cron: f.schedule_cron || f.cron_expr,
      operations: (f.operations ?? []).map((op, i) => {
        const synced = withSyncedAction(op)
        return {
          ...synced,
          flow_id: f.id,
          sort_order: op.sort_order ?? i,
        }
      }),
    }
    const exists = items.value.some((x) => x.id === f.id)
    if (exists) {
      await api.put(`/api/v1/flows/${encodeURIComponent(f.id)}`, body)
    } else {
      await api.post('/api/v1/flows', body)
    }
    await load()
  }

  async function add(f: Flow) {
    await save(f)
  }

  async function remove(id: string) {
    await api.del(`/api/v1/flows/${encodeURIComponent(id)}`)
    const next = { ...runStatus.value }
    delete next[id]
    runStatus.value = next
    const nextLog = { ...runLog.value }
    delete nextLog[id]
    runLog.value = nextLog
    const nextErr = { ...lastError.value }
    delete nextErr[id]
    lastError.value = nextErr
    await load()
  }

  async function execute(id: string) {
    clearRunUi(id)
    setRunStatus(id, 'running')
    const idx = items.value.findIndex((f) => f.id === id)
    if (idx >= 0) {
      const ops = (items.value[idx].operations ?? []).map((op) => ({ ...op, status: 'idle' as const }))
      items.value = items.value.map((x, i) =>
        i === idx ? { ...x, status: 'running', operations: ops, last_error: undefined } : x,
      )
    }
    appendLog(id, { at: Date.now(), status: 'running', label: 'Flow' })
    ensurePoll()
    try {
      await api.post(`/api/v1/flows/${encodeURIComponent(id)}/execute`)
    } catch (e) {
      setRunStatus(id, 'failed')
      const msg = e instanceof Error ? e.message : String(e)
      lastError.value = { ...lastError.value, [id]: msg }
      appendLog(id, { at: Date.now(), status: 'failed', error: msg, label: 'Flow' })
      if (idx >= 0) {
        items.value = items.value.map((x, i) =>
          i === idx ? { ...x, status: 'failed', last_error: msg } : x,
        )
      }
      stopPollIfIdle()
      throw e
    }
  }

  async function stop(id: string) {
    setRunStatus(id, 'cancelling')
    appendLog(id, { at: Date.now(), status: 'cancelling', label: 'Flow' })
    const idx = items.value.findIndex((f) => f.id === id)
    if (idx >= 0) {
      items.value = items.value.map((x, i) => (i === idx ? { ...x, status: 'cancelling' } : x))
    }
    await api.post(`/api/v1/flows/${encodeURIComponent(id)}/stop`)
    ensurePoll()
  }

  /**
   * Apply a flow / operation runtime event from SSE or poll.
   * Operation-level "completed" must NOT mark the whole flow completed —
   * only flow-level events (no opId) set the terminal flow status.
   */
  function applyRunEvent(flowId: string, status: string, opId?: string, error?: string) {
    if (!flowId || !status) return
    const idx = items.value.findIndex((f) => f.id === flowId)

    // Skip duplicate lifecycle events (progress ticks used to re-fire "running").
    {
      const curFlow = runStatus.value[flowId] || (idx >= 0 ? items.value[idx].status : '') || 'idle'
      if (opId && idx >= 0) {
        const curOp = items.value[idx].operations?.find((o) => o.id === opId)?.status
        const flowOk =
          status === 'running' || status === 'cancelling'
            ? curFlow === 'running' || curFlow === 'cancelling'
            : status === 'completed'
              ? curFlow === 'running' || curFlow === 'cancelling' || curFlow === status
              : curFlow === status
        // For op-level "running"/"completed": skip if op already has that status
        // and flow lifecycle doesn't need updating.
        if (curOp === status && !error) {
          if (status === 'running' && (curFlow === 'running' || curFlow === 'cancelling')) return
          if (status === 'completed' && flowOk) return
          if (status === 'failed' || status === 'cancelled') {
            if (curFlow === status) return
          }
        }
      } else if (!opId && curFlow === status && !error) {
        return
      }
    }

    // Resolve op label for log.
    let label = 'Flow'
    if (opId && idx >= 0) {
      const ops = items.value[idx].operations ?? []
      const oi = ops.findIndex((o) => o.id === opId)
      label = oi >= 0 ? `Op #${oi + 1}` : `Op ${opId.slice(0, 8)}`
    } else if (opId) {
      label = `Op ${opId.slice(0, 8)}`
    }

    // Deduplicate consecutive identical log lines.
    const prevLog = runLog.value[flowId] ?? []
    const last = prevLog[prevLog.length - 1]
    if (!last || last.status !== status || last.opId !== opId || last.error !== error) {
      appendLog(flowId, {
        at: Date.now(),
        status,
        opId,
        error: error || undefined,
        label,
      })
    }

    // Don't store cancel/kill noise as lastError (UI shows "Stopped" instead).
    if (error && status !== 'cancelled' && status !== 'cancelling') {
      const lower = error.toLowerCase()
      if (
        !lower.includes('signal: killed') &&
        !lower.includes('context canceled') &&
        !lower.includes('task cancelled')
      ) {
        lastError.value = { ...lastError.value, [flowId]: error }
      }
    }
    if (status === 'cancelled' || status === 'cancelling') {
      const { [flowId]: _drop, ...rest } = lastError.value
      lastError.value = rest
      // Clear raw error on op sync snapshot too.
      const snap = opSyncStatus.value[flowId]
      if (snap?.error_message) {
        opSyncStatus.value = {
          ...opSyncStatus.value,
          [flowId]: { ...snap, error_message: undefined, status: 'cancelled' },
        }
      }
    }

    if (idx < 0) {
      if (!opId) {
        setRunStatus(flowId, status)
      } else if (status === 'running' || status === 'cancelling') {
        setRunStatus(flowId, status)
      } else if (status === 'failed' || status === 'cancelled') {
        setRunStatus(flowId, status)
      }
      if (status === 'running' || status === 'cancelling') ensurePoll()
      else stopPollIfIdle()
      return
    }

    let flowStatus = runStatus.value[flowId] || items.value[idx].status || 'idle'
    const ops = [...(items.value[idx].operations ?? [])]

    if (opId) {
      for (let i = 0; i < ops.length; i++) {
        if (ops[i].id !== opId) continue
        ops[i] = {
          ...ops[i],
          status,
          ...(error ? { last_error: error } : {}),
        }
        break
      }
      if (status === 'running') flowStatus = 'running'
      else if (status === 'cancelling') flowStatus = 'cancelling'
      else if (status === 'failed' || status === 'cancelled') flowStatus = status
    } else {
      flowStatus = status
      if (status === 'completed' || status === 'failed' || status === 'cancelled') {
        for (let i = 0; i < ops.length; i++) {
          if (ops[i].status === 'running' || ops[i].status === 'cancelling') {
            ops[i] = {
              ...ops[i],
              status: status === 'completed' ? 'completed' : status,
            }
          }
        }
      }
    }

    setRunStatus(flowId, flowStatus)
    items.value = items.value.map((x, i) =>
      i === idx
        ? {
            ...x,
            status: flowStatus,
            operations: ops,
            ...(error && !opId ? { last_error: error } : {}),
          }
        : x,
    )

    if (flowStatus === 'running' || flowStatus === 'cancelling') ensurePoll()
    else stopPollIfIdle()
  }

  return {
    items,
    runStatus,
    runLog,
    lastError,
    opSyncStatus,
    runningFlowIds,
    isFlowRunning,
    flowStatusOf,
    logOf,
    activeSyncOf,
    load,
    save,
    add,
    remove,
    execute,
    stop,
    applyRunEvent,
    applySyncProgress,
    emptyFlow,
    emptyOperation,
    error: api.error,
    loading: api.loading,
  }
})
