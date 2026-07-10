import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useOptimisticList } from '@gnas/ui-shared'
import { useApi } from '@/composables/useApi'
import type { Schedule } from '@/api/types'

export const useSchedulesStore = defineStore('schedules', () => {
  const api = useApi()
  const items = ref<Schedule[]>([])
  const { optimisticUpdate } = useOptimisticList<Schedule>(items, {
    rollbackMessage: () => 'Failed to update schedule, reverted.',
  })

  async function load() {
    items.value = (await api.get<Schedule[]>('/api/v1/schedules')) ?? []
  }

  async function add(s: Schedule) {
    await api.post('/api/v1/schedules', s)
    await load()
  }

  // Optimistic: flip the toggle immediately instead of waiting on a
  // round-trip + full list reload; rolls back with a toast if the server
  // rejects the change.
  async function enable(id: string) {
    await optimisticUpdate(
      (s) => s.id === id,
      (s) => ({ ...s, enabled: true }),
      () => api.post(`/api/v1/schedules/${id}/enable`),
    )
  }

  async function disable(id: string) {
    await optimisticUpdate(
      (s) => s.id === id,
      (s) => ({ ...s, enabled: false }),
      () => api.post(`/api/v1/schedules/${id}/disable`),
    )
  }

  async function remove(id: string) {
    await api.del(`/api/v1/schedules/${id}`)
    await load()
  }

  return { items, load, add, enable, disable, remove, error: api.error, loading: api.loading }
})
