import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useApi } from '@/composables/useApi'
import type { Flow } from '@/api/types'

export const useFlowsStore = defineStore('flows', () => {
  const api = useApi()
  const items = ref<Flow[]>([])

  async function load() {
    items.value = (await api.get<Flow[]>('/api/v1/flows')) ?? []
  }

  async function add(f: Flow) {
    await api.post('/api/v1/flows', f)
    await load()
  }

  async function remove(id: string) {
    await api.del(`/api/v1/flows/${id}`)
    await load()
  }

  return { items, load, add, remove, error: api.error, loading: api.loading }
})
