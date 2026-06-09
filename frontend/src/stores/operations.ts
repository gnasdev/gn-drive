import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useApi } from '@/composables/useApi'
import type { FileEntry, SyncTask, Profile } from '@/api/types'

export const useOperationsStore = defineStore('operations', () => {
  const api = useApi()
  const entries = ref<FileEntry[]>([])
  const path = ref('gdrive:/')
  const tasks = ref<SyncTask[]>([])
  const profiles = ref<Profile[]>([])
  const busy = ref(false)

  async function loadProfiles() {
    profiles.value = (await api.get<Profile[]>('/api/v1/profiles')) ?? []
  }

  async function browse(remotePath: string) {
    busy.value = true
    try {
      entries.value = (await api.get<FileEntry[]>(`/api/v1/operations/fs?remote=${encodeURIComponent(remotePath)}`)) ?? []
      path.value = remotePath
    } finally {
      busy.value = false
    }
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

  return { entries, path, tasks, profiles, busy, loadProfiles, browse, startSync, loadTasks, error: api.error, loading: api.loading }
})
