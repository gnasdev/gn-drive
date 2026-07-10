<script setup lang="ts">
import { useI18n } from 'vue-i18n'

const props = withDefaults(
  defineProps<{
    label?: string
  }>(),
  { label: '' },
)

const { t } = useI18n()
</script>

<template>
  <div class="flex flex-col items-center justify-center gap-3 py-8 text-text-muted" role="status">
    <div class="grid w-full max-w-md gap-2 px-4">
      <div class="h-3 animate-pulse rounded bg-surface-hover" />
      <div class="h-3 w-4/5 animate-pulse rounded bg-surface-hover" />
      <div class="h-3 w-3/5 animate-pulse rounded bg-surface-hover" />
    </div>
    <span class="text-xs">{{ label || t('common.loading') }}</span>
  </div>
</template>
