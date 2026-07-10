<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { useConfirmDialogState } from '@/composables/useConfirmDialog'
import { cn } from '@/lib/cn'

const { t } = useI18n()
const { state, accept, cancel } = useConfirmDialogState()
</script>

<template>
  <Teleport to="body">
    <div
      v-if="state.open"
      class="fixed inset-0 z-[100] flex items-center justify-center bg-black/60 p-4"
      role="dialog"
      aria-modal="true"
      data-testid="confirm-dialog"
      @click.self="cancel"
    >
      <div class="w-full max-w-[420px] rounded-[10px] border border-border bg-surface p-6 shadow-lg">
        <h3 class="m-0 mb-2 text-base font-semibold text-text">{{ state.title }}</h3>
        <p class="m-0 mb-5 text-[13px] leading-relaxed text-text-muted">{{ state.message }}</p>
        <div class="flex justify-end gap-2">
          <button type="button" class="btn-ghost" data-testid="confirm-cancel" @click="cancel">
            {{ state.cancelText || t('common.cancel') }}
          </button>
          <button
            type="button"
            :class="cn(
              state.confirmVariant === 'danger' ? 'btn-danger' : 'btn-primary',
              'px-3.5 py-1.5',
            )"
            data-testid="confirm-accept"
            @click="accept"
          >
            {{ state.confirmText || t('common.confirm') }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
