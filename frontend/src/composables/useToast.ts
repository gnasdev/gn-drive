import { reactive } from 'vue'

export type ToastKind = 'success' | 'error' | 'info'

export interface ToastItem {
  id: number
  kind: ToastKind
  message: string
}

const state = reactive({
  items: [] as ToastItem[],
})

let seq = 0
const DEFAULT_TTL = 3500

function push(kind: ToastKind, message: string, ttlMs = DEFAULT_TTL) {
  const id = ++seq
  state.items.push({ id, kind, message })
  window.setTimeout(() => dismiss(id), ttlMs)
}

function dismiss(id: number) {
  const i = state.items.findIndex((t) => t.id === id)
  if (i >= 0) state.items.splice(i, 1)
}

export function useToast() {
  return {
    success: (message: string) => push('success', message),
    error: (message: string) => push('error', message),
    info: (message: string) => push('info', message),
    dismiss,
  }
}

/** Shared list for ToastHost (same reactive bag as useToast). */
export function useToastState() {
  return state
}
