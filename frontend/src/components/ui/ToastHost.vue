<script setup lang="ts">
import { useToastState, useToast } from '@/composables/useToast'
import { cn } from '@/lib/cn'

const state = useToastState()
const { dismiss } = useToast()
</script>

<template>
  <Teleport to="body">
    <div
      class="pointer-events-none fixed bottom-4 right-4 z-[110] flex w-[min(360px,calc(100vw-2rem))] flex-col gap-2"
      data-testid="toast-host"
    >
      <div
        v-for="t in state.items"
        :key="t.id"
        :class="cn(
          'pointer-events-auto rounded-md border px-3 py-2.5 text-[13px] shadow-lg',
          t.kind === 'success' && 'border-success/30 bg-surface text-success',
          t.kind === 'error' && 'border-danger/30 bg-surface text-danger',
          t.kind === 'info' && 'border-accent/30 bg-surface text-text',
        )"
        role="status"
        @click="dismiss(t.id)"
      >
        {{ t.message }}
      </div>
    </div>
  </Teleport>
</template>
