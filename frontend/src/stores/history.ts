import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useApi } from '@/composables/useApi'
import type { HistoryEntry, HistoryStats } from '@/api/types'

export const useHistoryStore = defineStore('history', () => {
  const api = useApi()
  const entries = ref<HistoryEntry[]>([])
  const stats = ref<HistoryStats | null>(null)
  const total = ref(0)

  async function load(limit = 50, offset = 0, profile = '') {
    const path = `/api/v1/history?limit=${limit}&offset=${offset}` + (profile ? `&profile=${encodeURIComponent(profile)}` : '')
    entries.value = (await api.get<HistoryEntry[]>(path)) ?? []
    stats.value = await api.get<HistoryStats>('/api/v1/history/stats')
  }

  async function clear() {
    await api.del('/api/v1/history')
    await load()
  }

  return { entries, stats, total, load, clear, error: api.error, loading: api.loading }
})
