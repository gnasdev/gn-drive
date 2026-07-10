import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useApi } from '@/composables/useApi'
import type { Board } from '@/api/types'

export const useBoardsStore = defineStore('boards', () => {
  const api = useApi()
  const items = ref<Board[]>([])
  const lastRun = ref<Record<string, { run_id?: string; status?: string; error?: string }>>({})

  async function load() {
    const list = (await api.get<Board[]>('/api/v1/boards')) ?? []
    // List endpoint returns metadata only; hydrate nodes/edges for UI counts + execute readiness.
    items.value = await Promise.all(
      list.map(async (b) => {
        try {
          const full = await api.get<Board>(`/api/v1/boards/${b.id}`)
          return full ?? b
        } catch {
          return b
        }
      }),
    )
  }

  async function add(b: Board) {
    await api.post('/api/v1/boards', b)
    await load()
  }

  async function update(b: Board) {
    await api.put(`/api/v1/boards/${b.id}`, b)
    await load()
  }

  async function remove(id: string) {
    await api.del(`/api/v1/boards/${id}`)
    await load()
  }

  async function execute(id: string, stopOnError = true) {
    const r = await api.post<{ run_id: string; status: string }>(`/api/v1/boards/${id}/execute`, {
      stop_on_error: stopOnError,
    })
    lastRun.value[id] = { run_id: r?.run_id, status: r?.status ?? 'running' }
    return r
  }

  async function stop(id: string) {
    await api.post(`/api/v1/boards/${id}/stop`)
    lastRun.value[id] = { ...(lastRun.value[id] ?? {}), status: 'cancelling' }
  }

  return {
    items,
    lastRun,
    load,
    add,
    update,
    remove,
    execute,
    stop,
    error: api.error,
    loading: api.loading,
  }
})
