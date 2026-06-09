import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useApi } from '@/composables/useApi'
import type { Schedule } from '@/api/types'

export const useSchedulesStore = defineStore('schedules', () => {
  const api = useApi()
  const items = ref<Schedule[]>([])

  async function load() {
    items.value = (await api.get<Schedule[]>('/api/v1/schedules')) ?? []
  }

  async function add(s: Schedule) {
    await api.post('/api/v1/schedules', s)
    await load()
  }

  async function enable(id: string) {
    await api.post(`/api/v1/schedules/${id}/enable`)
    await load()
  }

  async function disable(id: string) {
    await api.post(`/api/v1/schedules/${id}/disable`)
    await load()
  }

  async function remove(id: string) {
    await api.del(`/api/v1/schedules/${id}`)
    await load()
  }

  return { items, load, add, enable, disable, remove, error: api.error, loading: api.loading }
})
