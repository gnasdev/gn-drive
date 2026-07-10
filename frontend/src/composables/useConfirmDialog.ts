import { reactive } from 'vue'

export interface ConfirmOptions {
  title: string
  message: string
  confirmText?: string
  cancelText?: string
  confirmVariant?: 'danger' | 'primary'
}

interface ConfirmRequest extends ConfirmOptions {
  resolve: (ok: boolean) => void
}

const state = reactive({
  open: false,
  title: '',
  message: '',
  confirmText: 'Confirm',
  cancelText: 'Cancel',
  confirmVariant: 'primary' as 'danger' | 'primary',
  resolve: null as null | ((ok: boolean) => void),
})

function close(ok: boolean) {
  const resolve = state.resolve
  state.open = false
  state.resolve = null
  resolve?.(ok)
}

export function useConfirmDialog() {
  function confirmDialog(opts: ConfirmOptions): Promise<boolean> {
    return new Promise((resolve) => {
      state.title = opts.title
      state.message = opts.message
      state.confirmText = opts.confirmText ?? 'Confirm'
      state.cancelText = opts.cancelText ?? 'Cancel'
      state.confirmVariant = opts.confirmVariant ?? 'primary'
      state.resolve = resolve
      state.open = true
    })
  }

  return { confirmDialog }
}

export function useConfirmDialogState() {
  return {
    state,
    accept: () => close(true),
    cancel: () => close(false),
  }
}
