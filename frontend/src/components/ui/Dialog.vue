<script setup lang="ts">
import { watch } from 'vue'
import { cn } from '@/lib/cn'

const open = defineModel<boolean>({ default: false })

const props = withDefaults(
  defineProps<{
    title?: string
    size?: 'sm' | 'md' | 'lg'
  }>(),
  {
    title: '',
    size: 'md',
  },
)

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') open.value = false
}

watch(open, (v) => {
  if (v) window.addEventListener('keydown', onKey)
  else window.removeEventListener('keydown', onKey)
})
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-[100] flex items-center justify-center bg-black/60 p-4"
      role="dialog"
      aria-modal="true"
      @click.self="open = false"
    >
      <div
        :class="cn(
          'w-full rounded-[10px] border border-border bg-surface p-6 shadow-lg',
          size === 'sm' && 'max-w-[460px]',
          size === 'md' && 'max-w-[560px]',
          size === 'lg' && 'max-w-[720px]',
        )"
      >
        <h3 v-if="title" class="m-0 mb-2 text-base font-semibold text-text">{{ title }}</h3>
        <div class="text-[13px] text-text-muted">
          <slot />
        </div>
      </div>
    </div>
  </Teleport>
</template>
