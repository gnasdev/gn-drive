// Composable wrapper around the raw `api` client. Adds:
//   - typed methods
//   - shared loading/error state per request
//   - request cancellation
//
// Pages can use either `useApi()` (singleton) or call `api.get/post/...` directly.

import { ref, type Ref } from 'vue'
import { api as rawApi, type ApiError } from '@/api/client'

export interface UseApi {
  loading: Ref<boolean>
  error: Ref<string | null>
  get: <T = unknown>(path: string) => Promise<T>
  post: <T = unknown>(path: string, body?: unknown) => Promise<T>
  put: <T = unknown>(path: string, body?: unknown) => Promise<T>
  del: <T = unknown>(path: string) => Promise<T>
}

export function useApi(): UseApi {
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function wrap<T>(fn: () => Promise<T>): Promise<T> {
    loading.value = true
    error.value = null
    try {
      return await fn()
    } catch (e) {
      const err = e as ApiError
      error.value = err?.message ?? 'request failed'
      throw err
    } finally {
      loading.value = false
    }
  }

  return {
    loading,
    error,
    get: <T = unknown>(path: string) => wrap<T>(() => rawApi.get<T>(path)),
    post: <T = unknown>(path: string, body?: unknown) => wrap<T>(() => rawApi.post<T>(path, body)),
    put: <T = unknown>(path: string, body?: unknown) => wrap<T>(() => rawApi.put<T>(path, body)),
    del: <T = unknown>(path: string) => wrap<T>(() => rawApi.delete<T>(path)),
  }
}
