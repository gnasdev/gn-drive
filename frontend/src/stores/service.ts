import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useApi } from '@/composables/useApi'
import type { ServiceStatus } from '@/api/types'

export const useServiceStore = defineStore('service', () => {
  const api = useApi()
  const status = ref<ServiceStatus | null>(null)
  const lastOutput = ref('')
  const busy = ref(false)

  async function load() {
    status.value = (await api.get<ServiceStatus>('/api/v1/service/status')) ?? null
  }

  async function run(action: 'install' | 'uninstall' | 'start' | 'stop' | 'restart') {
    busy.value = true
    lastOutput.value = ''
    try {
      const r = await api.post<{ ok: boolean; output: string }>(`/api/v1/service/${action}`)
      lastOutput.value = r?.output ?? ''
      await load()
    } catch (e: any) {
      lastOutput.value = e?.message ?? 'failed'
    } finally {
      busy.value = false
    }
  }

  return {
    status,
    lastOutput,
    busy,
    load,
    install: () => run('install'),
    uninstall: () => run('uninstall'),
    start: () => run('start'),
    stop: () => run('stop'),
    restart: () => run('restart'),
    error: api.error,
    loading: api.loading,
  }
})
