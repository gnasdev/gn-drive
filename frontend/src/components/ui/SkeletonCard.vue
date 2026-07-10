<script setup lang="ts">
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    count?: number
    showImage?: boolean
  }>(),
  {
    count: 1,
    showImage: true,
  },
)

const items = computed(() => Array.from({ length: props.count }, (_, i) => i))
</script>

<template>
  <template v-for="i in items" :key="i">
    <div class="card animate-pulse p-4">
      <div v-if="showImage" class="mb-3 h-24 rounded-md bg-surface-hover" />
      <div class="mb-2 h-3 w-1/3 rounded bg-surface-hover" />
      <div class="mb-2 h-7 w-1/2 rounded bg-surface-hover" />
      <div class="h-3 w-2/3 rounded bg-surface-hover" />
    </div>
  </template>
</template>
