import type { Ref } from 'vue'
import { useToast } from './useToast'

export interface UseOptimisticListOptions {
  rollbackMessage?: () => string
}

export function useOptimisticList<T>(
  items: Ref<T[]>,
  options: UseOptimisticListOptions = {},
) {
  const toast = useToast()

  async function optimisticUpdate(
    predicate: (item: T) => boolean,
    updater: (item: T) => T,
    serverCall: () => Promise<unknown>,
  ): Promise<void> {
    const index = items.value.findIndex(predicate)
    if (index < 0) {
      await serverCall()
      return
    }
    const previous = items.value[index]
    const next = updater(previous)
    const copy = items.value.slice()
    copy[index] = next
    items.value = copy
    try {
      await serverCall()
    } catch (e) {
      const rollback = items.value.slice()
      const still = rollback.findIndex(predicate)
      if (still >= 0) {
        rollback[still] = previous
        items.value = rollback
      }
      const msg =
        options.rollbackMessage?.() ??
        (e instanceof Error ? e.message : 'Update failed, reverted.')
      toast.error(msg)
      throw e
    }
  }

  return { optimisticUpdate }
}
