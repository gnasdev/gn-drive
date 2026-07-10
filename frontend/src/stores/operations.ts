import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useApi } from '@/composables/useApi'
import type { FileEntry, SyncTask, Profile } from '@/api/types'

export type FileOp = 'copy' | 'move' | 'check' | 'mkdir' | 'purge' | 'delete'

export const useOperationsStore = defineStore('operations', () => {
  const api = useApi()
  const entries = ref<FileEntry[]>([])
  const path = ref('gdrive:/')
  const tasks = ref<SyncTask[]>([])
  const profiles = ref<Profile[]>([])
  const busy = ref(false)
  const lastOpResult = ref<string | null>(null)

  async function loadProfiles() {
    profiles.value = (await api.get<Profile[]>('/api/v1/profiles')) ?? []
  }

  async function browse(remotePath: string) {
    busy.value = true
    api.error.value = null
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

  async function runOp(op: FileOp, opts: { source?: string; dest?: string; path?: string }) {
    busy.value = true
    lastOpResult.value = null
    api.error.value = null
    try {
      const r = await api.post<{ ok: boolean; op: string }>('/api/v1/operations', {
        op,
        source: opts.source,
        dest: opts.dest,
        path: opts.path,
      })
      lastOpResult.value = `${r?.op ?? op} ok`
      return r
    } finally {
      busy.value = false
    }
  }

  return {
    entries,
    path,
    tasks,
    profiles,
    busy,
    lastOpResult,
    loadProfiles,
    browse,
    startSync,
    loadTasks,
    runOp,
    error: api.error,
    loading: api.loading,
  }
})
