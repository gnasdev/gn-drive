/**
 * Subscribe to backend SSE (/api/v1/events) and fan out sync / flow topics
 * into the flows Pinia store so the Workspace UI updates live during runs.
 */
import { onMounted, onUnmounted, ref, watch, type Ref } from 'vue'
import { useFlowsStore } from '@/stores/flows'

const SYNC_TOPICS = ['sync:started', 'sync:progress', 'sync:completed', 'sync:failed'] as const
/** Preferred topic for flow runs. */
const FLOW_RUN_TOPIC = 'flow:execution'
/** Legacy topic still emitted by flowengine (board_id = flow id). */
const LEGACY_FLOW_TOPIC = 'board:execution'

export type UseEventStreamOptions = {
  /** When provided, connect only while this is true (e.g. auth.unlocked). */
  enabled?: Ref<boolean>
}

export function useEventStream(opts: UseEventStreamOptions = {}) {
  const flows = useFlowsStore()
  const connected = ref(false)
  let es: EventSource | null = null
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null
  let intentionalClose = false

  function applyFlowPayload(data: Record<string, unknown>) {
    const id = String(data.flow_id ?? data.board_id ?? '')
    const status = String(data.status ?? '')
    const opId = data.op_id
      ? String(data.op_id)
      : data.node_id
        ? String(data.node_id)
        : undefined
    // action field is reused as error text on board:execution; only treat as
    // error when status is failed (push/pull actions would otherwise poison UI).
    let error: string | undefined
    if (data.error) error = String(data.error)
    else if (status === 'failed' && data.action) error = String(data.action)
    if (!id || !status) return
    flows.applyRunEvent(id, status, opId || undefined, error)
  }

  function handleMessage(topic: string, raw: string) {
    let data: Record<string, unknown> = {}
    try {
      data = JSON.parse(raw) as Record<string, unknown>
    } catch {
      return
    }
    if ((SYNC_TOPICS as readonly string[]).includes(topic)) {
      // Flow path-sync uses busyKey profile_id = `${flowId}:${opId}`.
      flows.applySyncProgress(topic, data)
      return
    }
    if (topic === FLOW_RUN_TOPIC || topic === LEGACY_FLOW_TOPIC) {
      applyFlowPayload(data)
    }
  }

  function connect() {
    intentionalClose = false
    if (es) {
      es.close()
      es = null
    }
    // Same-origin EventSource sends cookies automatically.
    es = new EventSource('/api/v1/events')
    es.onopen = () => {
      connected.value = true
    }
    es.onerror = () => {
      connected.value = false
      es?.close()
      es = null
      if (intentionalClose) return
      if (reconnectTimer) clearTimeout(reconnectTimer)
      reconnectTimer = setTimeout(connect, 2_000)
    }

    for (const topic of SYNC_TOPICS) {
      es.addEventListener(topic, (ev) => {
        handleMessage(topic, (ev as MessageEvent).data)
      })
    }
    for (const topic of [FLOW_RUN_TOPIC, LEGACY_FLOW_TOPIC]) {
      es.addEventListener(topic, (ev) => {
        handleMessage(topic, (ev as MessageEvent).data)
      })
    }
  }

  function disconnect() {
    intentionalClose = true
    if (reconnectTimer) {
      clearTimeout(reconnectTimer)
      reconnectTimer = null
    }
    es?.close()
    es = null
    connected.value = false
  }

  if (opts.enabled) {
    watch(
      opts.enabled,
      (on) => {
        if (on) connect()
        else disconnect()
      },
      { immediate: true },
    )
  } else {
    onMounted(() => {
      connect()
    })
  }

  onUnmounted(() => {
    disconnect()
  })

  return { connected, connect, disconnect }
}
