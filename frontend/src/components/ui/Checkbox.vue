<script setup lang="ts">
const model = defineModel<boolean>({ default: false })

defineProps<{
  label?: string
  disabled?: boolean
}>()
</script>

<template>
  <label class="inline-flex cursor-pointer items-center gap-2 text-[13px] text-text">
    <input
      v-model="model"
      type="checkbox"
      class="h-3.5 w-3.5 rounded border-border accent-accent"
      :disabled="disabled"
    />
    <span v-if="label">{{ label }}</span>
    <slot v-else />
  </label>
</template>
