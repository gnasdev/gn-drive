import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useApi } from '@/composables/useApi'
import type { Remote } from '@/api/types'

export const useRemotesStore = defineStore('remotes', () => {
  const api = useApi()
  const items = ref<Remote[]>([])

  async function load() {
    items.value = (await api.get<Remote[]>('/api/v1/remotes')) ?? []
  }

  async function add(name: string, type: string, config: string[] = []) {
    const r = await api.post<Remote>('/api/v1/remotes', { name, type, config })
    await load()
    return r
  }

  async function remove(name: string) {
    await api.del(`/api/v1/remotes/${encodeURIComponent(name)}`)
    await load()
  }

  async function test(name: string): Promise<{ ok: boolean; error?: string }> {
    try {
      await api.post(`/api/v1/remotes/${encodeURIComponent(name)}/test`)
      return { ok: true }
    } catch (e: any) {
      return { ok: false, error: e?.message ?? 'test failed' }
    }
  }

  return { items, load, add, remove, test, error: api.error, loading: api.loading }
})
