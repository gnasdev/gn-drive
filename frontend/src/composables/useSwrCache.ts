import { onMounted, ref, type Ref } from 'vue'

export type SwrState = 'idle' | 'hydrating' | 'fresh' | 'stale' | 'error'

export interface UseSwrCacheOptions<T> {
  namespace: string
  key: string
  userScope: () => string
  ttlMs: number
  fetcher: () => Promise<T>
}

interface CacheEnvelope<T> {
  data: T
  cachedAt: number
}

function storageKey(namespace: string, scope: string, key: string) {
  return `${namespace}:${scope}:${key}`
}

export function useSwrCache<T>(opts: UseSwrCacheOptions<T>): {
  data: Ref<T | null>
  state: Ref<SwrState>
  error: Ref<unknown>
  refresh: () => Promise<void>
} {
  const data = ref<T | null>(null) as Ref<T | null>
  const state = ref<SwrState>('idle')
  const error = ref<unknown>(null)

  function readCache(): CacheEnvelope<T> | null {
    try {
      const raw = localStorage.getItem(
        storageKey(opts.namespace, opts.userScope(), opts.key),
      )
      if (!raw) return null
      return JSON.parse(raw) as CacheEnvelope<T>
    } catch {
      return null
    }
  }

  function writeCache(value: T) {
    try {
      const envelope: CacheEnvelope<T> = { data: value, cachedAt: Date.now() }
      localStorage.setItem(
        storageKey(opts.namespace, opts.userScope(), opts.key),
        JSON.stringify(envelope),
      )
    } catch {
      // quota / private mode
    }
  }

  async function refresh() {
    const hadData = data.value != null
    if (!hadData) state.value = 'hydrating'
    else state.value = 'stale'
    error.value = null
    try {
      const next = await opts.fetcher()
      data.value = next
      writeCache(next)
      state.value = 'fresh'
    } catch (e) {
      error.value = e
      // Stale-while-revalidate: keep previous data on refresh failure.
      state.value = hadData || data.value != null ? 'stale' : 'error'
    }
  }

  onMounted(() => {
    const cached = readCache()
    if (cached) {
      data.value = cached.data
      const age = Date.now() - cached.cachedAt
      state.value = age < opts.ttlMs ? 'fresh' : 'stale'
    }
    void refresh()
  })

  return { data, state, error, refresh }
}
