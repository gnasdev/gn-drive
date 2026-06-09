import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useApi } from '@/composables/useApi'
import type { Profile } from '@/api/types'

export const useProfilesStore = defineStore('profiles', () => {
  const api = useApi()
  const items = ref<Profile[]>([])

  async function load() {
    items.value = (await api.get<Profile[]>('/api/v1/profiles')) ?? []
  }

  async function add(p: Profile) {
    await api.post('/api/v1/profiles', p)
    await load()
  }

  async function update(p: Profile) {
    await api.put(`/api/v1/profiles/${encodeURIComponent(p.name)}`, p)
    await load()
  }

  async function remove(name: string) {
    await api.del(`/api/v1/profiles/${encodeURIComponent(name)}`)
    await load()
  }

  return { items, load, add, update, remove, error: api.error, loading: api.loading }
})
