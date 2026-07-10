import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { api } from '@/api/client'

export interface AppStatus {
  setup: boolean
  /** Process crypto unlocked AND (when setup) a valid web session cookie. */
  unlocked: boolean
  /** Valid gn-drive-session cookie present (minted on unlock or /status resume). */
  session?: boolean
  version: string
  lockout?: {
    failed_attempts: number
    locked_until: string
    is_locked: boolean
    retry_after_secs: number
  }
}

export const useAuthStore = defineStore('auth', () => {
  const initialized = ref(false)
  const setup = ref(false)
  const unlocked = ref(false)
  const version = ref('dev')
  const lockout = ref<AppStatus['lockout'] | null>(null)
  const busy = ref(false)
  const error = ref<string | null>(null)

  async function fetchStatus() {
    try {
      // credentials: same-origin so Set-Cookie from session resume is stored.
      const s = await api.get<AppStatus>('/api/v1/status')
      setup.value = s.setup
      unlocked.value = s.unlocked
      version.value = s.version
      lockout.value = s.lockout ?? null
    } catch (e: any) {
      error.value = e?.message ?? 'failed to fetch status'
      unlocked.value = false
    } finally {
      initialized.value = true
    }
  }

  async function doSetup(password: string) {
    busy.value = true
    error.value = null
    try {
      await api.post('/api/v1/auth/setup', { password })
      // Response Set-Cookie creates HttpOnly session; refresh gate state from /status.
      await fetchStatus()
      if (!unlocked.value) {
        // Fallback if status race: setup response already proved success.
        setup.value = true
        unlocked.value = true
      }
    } catch (e: any) {
      error.value = e?.message ?? 'setup failed'
      throw e
    } finally {
      busy.value = false
    }
  }

  async function unlock(password: string) {
    busy.value = true
    error.value = null
    try {
      await api.post('/api/v1/auth/unlock', { password })
      // Unlock mints gn-drive-session cookie; /status confirms session+unlocked.
      await fetchStatus()
      if (!unlocked.value) {
        unlocked.value = true
      }
    } catch (e: any) {
      error.value = e?.message ?? 'unlock failed'
      throw e
    } finally {
      busy.value = false
    }
  }

  async function lock() {
    busy.value = true
    error.value = null
    try {
      await api.post('/api/v1/auth/lock')
      unlocked.value = false
    } catch (e: any) {
      error.value = e?.message ?? 'lock failed'
      throw e
    } finally {
      busy.value = false
    }
  }

  const canAccess = computed(() => unlocked.value)

  return {
    initialized,
    setup,
    unlocked,
    version,
    lockout,
    busy,
    error,
    canAccess,
    fetchStatus,
    doSetup,
    unlock,
    lock,
  }
})
