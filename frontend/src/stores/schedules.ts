import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useOptimisticList } from '@/composables/useOptimisticList'
import { useApi } from '@/composables/useApi'
import type { Schedule } from '@/api/types'
import { i18n } from '@/i18n'

export const useSchedulesStore = defineStore('schedules', () => {
  const api = useApi()
  const items = ref<Schedule[]>([])
  const { optimisticUpdate } = useOptimisticList<Schedule>(items, {
    rollbackMessage: () => i18n.global.t('schedules.rollback'),
  })

  async function load() {
    items.value = (await api.get<Schedule[]>('/api/v1/schedules')) ?? []
  }

  async function add(s: Schedule) {
    await api.post('/api/v1/schedules', s)
    await load()
  }

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
