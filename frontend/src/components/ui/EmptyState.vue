<script setup lang="ts">
withDefaults(
  defineProps<{
    title: string
    description?: string
    message?: string
  }>(),
  {
    description: '',
    message: '',
  },
)
</script>

<template>
  <div class="flex flex-col items-center justify-center gap-2 px-6 py-10 text-center text-text-dim">
    <div v-if="$slots.icon" class="text-text-muted">
      <slot name="icon" />
    </div>
    <p class="m-0 text-sm font-medium text-text-muted">{{ title }}</p>
    <p v-if="description || message" class="m-0 max-w-md text-[13px] text-text-dim">
      {{ description || message }}
    </p>
    <slot />
  </div>
</template>
