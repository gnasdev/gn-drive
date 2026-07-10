<script setup lang="ts">
import { computed } from 'vue'
import { cn } from '@/lib/cn'

const props = withDefaults(
  defineProps<{
    type?: 'info' | 'success' | 'error'
    message?: string
  }>(),
  { type: 'info' },
)

const classes = computed(() =>
  cn(
    'mb-3 rounded-md px-3 py-2 text-xs',
    props.type === 'error' && 'bg-danger/10 text-danger',
    props.type === 'success' && 'bg-success/10 text-success',
    props.type === 'info' && 'bg-accent/10 text-accent',
  ),
)
</script>

<template>
  <div :class="classes" role="alert">
    <slot>{{ message }}</slot>
  </div>
</template>
