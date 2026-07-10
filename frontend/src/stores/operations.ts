import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useApi } from '@/composables/useApi'
import type { SyncTask, Profile } from '@/api/types'

export const useOperationsStore = defineStore('operations', () => {
  const api = useApi()
  const tasks = ref<SyncTask[]>([])
  const profiles = ref<Profile[]>([])

  async function loadProfiles() {
    profiles.value = (await api.get<Profile[]>('/api/v1/profiles')) ?? []
  }

  async function startSync(action: string, profileName: string): Promise<string | null> {
    try {
      const r = await api.post<{ task_id: string }>('/api/v1/sync', { action, profile_name: profileName })
      return r?.task_id ?? null
    } catch {
      return null
    }
  }

  async function loadTasks() {
    tasks.value = (await api.get<SyncTask[]>('/api/v1/sync/tasks')) ?? []
  }

  return {
    tasks,
    profiles,
    loadProfiles,
    startSync,
    loadTasks,
    error: api.error,
    loading: api.loading,
  }
})
