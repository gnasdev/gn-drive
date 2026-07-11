import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { useApi } from '@/composables/useApi'
import type { SyncTask, Profile } from '@/api/types'

/** How long to keep a finished task visible after it leaves the engine. */
const RECENT_TTL_MS = 8_000

export const useOperationsStore = defineStore('operations', () => {
  const api = useApi()
  /** Live tasks from the engine (running). */
  const tasks = ref<SyncTask[]>([])
  /** Recently completed/failed tasks kept for UI feedback. */
  const recent = ref<SyncTask[]>([])
  const profiles = ref<Profile[]>([])

  const activeTasks = computed(() =>
    tasks.value.filter((x) => x.status === 'running' || x.status === 'pending'),
  )

  /** Profile names currently running a sync. */
  const runningProfileNames = computed(() => {
    const set = new Set<string>()
    for (const t of activeTasks.value) {
      if (t.name) set.add(t.name)
    }
    return set
  })

  function isProfileRunning(name: string): boolean {
    return runningProfileNames.value.has(name)
  }

  async function loadProfiles() {
    profiles.value = (await api.get<Profile[]>('/api/v1/profiles')) ?? []
  }

  async function startSync(action: string, profileName: string): Promise<string | null> {
    try {
      const r = await api.post<{ task_id: string }>('/api/v1/sync', {
        action,
        profile_name: profileName,
      })
      const id = r?.task_id ?? null
      if (id) {
        // Optimistic row so the UI updates before the next poll/SSE.
        const optimistic: SyncTask = {
          id,
          name: profileName,
          action,
          status: 'running',
          started_at: new Date().toISOString(),
          stats: {},
        }
        if (!tasks.value.some((t) => t.id === id)) {
          tasks.value = [optimistic, ...tasks.value]
        }
        // Immediate refresh for real stats.
        void loadTasks()
      }
      return id
    } catch {
      return null
    }
  }

  async function stopTask(id: string): Promise<boolean> {
    try {
      await api.del(`/api/v1/sync/tasks/${encodeURIComponent(id)}`)
      const t = tasks.value.find((x) => x.id === id)
      if (t) {
        t.status = 'cancelled'
        pushRecent({ ...t })
      }
      void loadTasks()
      return true
    } catch {
      return false
    }
  }

  async function loadTasks() {
    const list = (await api.get<SyncTask[]>('/api/v1/sync/tasks')) ?? []
    // Detect tasks that disappeared (finished) so we can flash recent state.
    const nextIds = new Set(list.map((t) => t.id))
    for (const prev of tasks.value) {
      if (!nextIds.has(prev.id) && (prev.status === 'running' || prev.status === 'pending')) {
        // Engine dropped the task — treat as completed unless we already marked cancel.
        pushRecent({
          ...prev,
          status: 'completed',
          ended_at: new Date().toISOString(),
        })
      }
    }
    tasks.value = list
  }

  function pushRecent(task: SyncTask) {
    recent.value = [task, ...recent.value.filter((t) => t.id !== task.id)].slice(0, 12)
    window.setTimeout(() => {
      recent.value = recent.value.filter((t) => t.id !== task.id)
    }, RECENT_TTL_MS)
  }

  /** Apply SSE sync progress / terminal events. */
  function applySyncEvent(topic: string, data: Record<string, unknown>) {
    const taskId = String(data.task_id ?? '')
    if (!taskId) return
    const profile = String(data.profile_id ?? '')
    const action = String(data.action ?? '')

    if (topic === 'sync:started') {
      if (!tasks.value.some((t) => t.id === taskId)) {
        tasks.value = [
          {
            id: taskId,
            name: profile,
            action,
            status: 'running',
            started_at: new Date().toISOString(),
            stats: {},
          },
          ...tasks.value,
        ]
      }
      return
    }

    if (topic === 'sync:progress') {
      const idx = tasks.value.findIndex((t) => t.id === taskId)
      const next: SyncTask = {
        id: taskId,
        name: profile || tasks.value[idx]?.name || '',
        action: action || tasks.value[idx]?.action || '',
        status: 'running',
        started_at: tasks.value[idx]?.started_at,
        stats: {
          bytes: Number(data.transferred ?? 0),
          bytes_total: Number(data.total ?? 0),
          files: Number(data.files_transferred ?? 0),
          files_total: Number(data.total_files ?? 0),
          errors: Number(data.errors ?? 0),
          speed_bps: Number(data.bytes_per_sec ?? 0),
          eta_secs: Number(data.eta_secs ?? 0),
          current_file: String(data.current_file ?? ''),
        },
      }
      if (idx >= 0) {
        const copy = tasks.value.slice()
        copy[idx] = { ...copy[idx], ...next }
        tasks.value = copy
      } else {
        tasks.value = [next, ...tasks.value]
      }
      return
    }

    if (topic === 'sync:completed' || topic === 'sync:failed') {
      const status = topic === 'sync:completed' ? 'completed' : 'failed'
      const prev = tasks.value.find((t) => t.id === taskId)
      const finished: SyncTask = {
        id: taskId,
        name: profile || prev?.name || '',
        action: action || prev?.action || '',
        status,
        started_at: prev?.started_at,
        ended_at: new Date().toISOString(),
        stats: prev?.stats,
        errors: Number(data.errors ?? prev?.stats?.errors ?? 0),
        error_message: data.error_message ? String(data.error_message) : undefined,
        transferred: data.bytes != null ? Number(data.bytes) : prev?.stats?.bytes,
      }
      tasks.value = tasks.value.filter((t) => t.id !== taskId)
      pushRecent(finished)
      // Confirm with server list.
      void loadTasks()
    }
  }

  return {
    tasks,
    recent,
    activeTasks,
    runningProfileNames,
    isProfileRunning,
    profiles,
    loadProfiles,
    startSync,
    stopTask,
    loadTasks,
    applySyncEvent,
    error: api.error,
    loading: api.loading,
  }
})
