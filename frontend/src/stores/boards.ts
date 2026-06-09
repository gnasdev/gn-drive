import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useApi } from '@/composables/useApi'
import type { Board } from '@/api/types'

export const useBoardsStore = defineStore('boards', () => {
  const api = useApi()
  const items = ref<Board[]>([])

  async function load() {
    items.value = (await api.get<Board[]>('/api/v1/boards')) ?? []
  }

  async function add(b: Board) {
    await api.post('/api/v1/boards', b)
    await load()
  }

  async function remove(id: string) {
    await api.del(`/api/v1/boards/${id}`)
    await load()
  }

  return { items, load, add, remove, error: api.error, loading: api.loading }
})
